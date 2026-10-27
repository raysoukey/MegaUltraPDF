// SPDX-License-Identifier: GPL-3.0-or-later
//! Passe `lopdf` appliquée aux octets **avant l'écriture** d'un fichier (ADR-0010).
//!
//! PDFium laisse des traces que les autres lecteurs ne devraient pas voir : une clé privée
//! `/PDFIUM_HasGeneratedAP` et une apparence imposée aux notes (icône jaune fixe), des
//! objets orphelins créés à chaque rendu, et nos formes `Ink` n'ont pas l'`/InkList` que la
//! norme exige (ADR-0008). On corrige tout cela sur l'arbre d'objets, sans toucher au document
//! PDFium en mémoire.
//!
//! \[Apprentissage\] **Flux de contenu** : l'apparence d'une annotation est un petit programme
//! (`m` déplacer, `l` ligne, `c` courbe, `re` rectangle…). `lopdf::content::Content::decode`
//! le découpe en opérations ; on en déduit la liste de points attendue dans `/InkList`.

use std::collections::BTreeSet;

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::error::{CoreError, CoreResult};

fn structure(e: impl std::fmt::Display) -> CoreError {
    CoreError::InvalidOperation(format!("nettoyage avant enregistrement : {e}"))
}

/// Octets prêts à écrire : notes sans apparence imposée, `/InkList` sur les formes, objets
/// orphelins élagués et renumérotés. Un document chiffré est rendu tel quel.
pub fn before_save(bytes: &[u8]) -> CoreResult<Vec<u8>> {
    let mut doc = match Document::load_mem(bytes) {
        Ok(doc) => doc,
        // Un fichier que lopdf ne relit pas est écrit tel que PDFium l'a produit.
        Err(_) => return Ok(bytes.to_vec()),
    };
    // `is_encrypted()` ne voit que le dictionnaire `/Encrypt` ; or lopdf **déchiffre en
    // silence** un fichier dont le mot de passe utilisateur est vide (protection « propriétaire »
    // seule) et retire `/Encrypt` : `was_encrypted()` garde la trace. Dans les deux cas, on ne
    // réécrit pas : le fichier resterait lisible mais perdrait sa protection.
    if doc.is_encrypted() || doc.was_encrypted() {
        return Ok(bytes.to_vec());
    }
    let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
    for page_id in &pages {
        clean_page_annotations(&mut doc, *page_id)?;
        identify_annotations(&mut doc, *page_id)?;
    }
    drop_dangling_annotation_links(&mut doc, &pages)?;
    // Le fichier est réécrit : sa date de modification aussi (C12, Lot 9).
    crate::properties::stamp_mod_date(&mut doc);
    let pages_before = doc.get_pages().len();
    doc.prune_objects();
    doc.renumber_objects();
    let mut out = Vec::new();
    doc.save_to(&mut out)?;
    // Ceinture de sécurité : la réécriture `lopdf` remplace le fichier de l'utilisateur ; si
    // elle ne se relit pas avec le même nombre de pages, on écrit les octets PDFium tels quels.
    if !rewrite_is_sane(pages_before, &out) {
        return Ok(bytes.to_vec());
    }
    Ok(out)
}

/// Vrai si `out` se relit comme un PDF non vide ayant encore `pages_before` pages.
pub(crate) fn rewrite_is_sane(pages_before: usize, out: &[u8]) -> bool {
    match Document::load_mem(out) {
        Ok(reloaded) => {
            reloaded.get_pages().len() == pages_before && reloaded.trailer.get(b"Root").is_ok()
        }
        Err(_) => false,
    }
}

/// Nettoie chaque annotation d'une page. PDFium écrit une annotation fraîchement créée
/// **directement** dans le tableau `/Annots` de la page, et sous forme de référence après
/// un rendu : les deux cas sont traités, et le tableau n'est réécrit que s'il a changé.
fn clean_page_annotations(doc: &mut Document, page_id: ObjectId) -> CoreResult<()> {
    let annots = doc
        .get_dictionary(page_id)
        .ok()
        .and_then(|page| page.get(b"Annots").ok().cloned());
    let Some(annots) = annots else {
        return Ok(());
    };
    // Le tableau peut lui-même être un objet indirect : on retient où le réécrire.
    let (array_id, array) = match &annots {
        Object::Reference(id) => (
            Some(*id),
            doc.get_object(*id)
                .ok()
                .and_then(|o| o.as_array().ok())
                .cloned(),
        ),
        Object::Array(array) => (None, Some(array.clone())),
        _ => (None, None),
    };
    let Some(mut array) = array else {
        return Ok(());
    };
    let mut changed = false;
    for item in array.iter_mut() {
        match item {
            Object::Reference(id) => {
                let id = *id;
                let cleaned = doc.get_dictionary(id).ok().and_then(|d| cleaned(doc, d));
                if let Some(new_dict) = cleaned {
                    *doc.get_dictionary_mut(id).map_err(structure)? = new_dict;
                }
            }
            Object::Dictionary(dict) => {
                if let Some(new_dict) = cleaned(doc, dict) {
                    *dict = new_dict;
                    changed = true;
                }
            }
            _ => {}
        }
    }
    if changed {
        match array_id {
            Some(id) => {
                doc.objects.insert(id, Object::Array(array));
            }
            None => doc
                .get_dictionary_mut(page_id)
                .map_err(structure)?
                .set("Annots", Object::Array(array)),
        }
    }
    Ok(())
}

/// Le tableau `/Annots` d'une page : où le réécrire (objet indirect, ou `None` s'il est
/// direct dans la page) et ses éléments.
fn page_annots(doc: &Document, page_id: ObjectId) -> Option<(Option<ObjectId>, Vec<Object>)> {
    match doc.get_dictionary(page_id).ok()?.get(b"Annots").ok()? {
        Object::Reference(id) => Some((
            Some(*id),
            doc.get_object(*id).ok()?.as_array().ok()?.clone(),
        )),
        Object::Array(array) => Some((None, array.clone())),
        _ => None,
    }
}

/// Donne à chaque annotation de la page qui n'en a pas un `/NM` (nom unique sur la page)
/// et un `/P` (sa page) : clés facultatives que PDFium n'écrit pas, mais dont se servent les
/// lecteurs qui synchronisent, fusionnent ou répondent à des commentaires (§12.5.2, B3).
fn identify_annotations(doc: &mut Document, page_id: ObjectId) -> CoreResult<()> {
    let Some((array_id, mut items)) = page_annots(doc, page_id) else {
        return Ok(());
    };
    let mut names: BTreeSet<Vec<u8>> = items
        .iter()
        .filter_map(|item| match item {
            Object::Reference(id) => doc.get_dictionary(*id).ok(),
            Object::Dictionary(dict) => Some(dict),
            _ => None,
        })
        .filter_map(|d| {
            d.get(b"NM")
                .and_then(Object::as_str)
                .ok()
                .map(<[u8]>::to_vec)
        })
        .collect();
    let mut inline_changed = false;
    for (i, item) in items.iter_mut().enumerate() {
        let mut fresh = || {
            let mut name = format!("mega-{}-{}-{i}", page_id.0, page_id.1).into_bytes();
            while names.contains(&name) {
                name.push(b'x');
            }
            names.insert(name.clone());
            Object::String(name, lopdf::StringFormat::Literal)
        };
        let dict = match item {
            Object::Reference(id) => match doc.get_dictionary_mut(*id) {
                Ok(dict) => dict,
                Err(_) => continue,
            },
            Object::Dictionary(dict) => {
                inline_changed |= !dict.has(b"NM") || !dict.has(b"P");
                dict
            }
            _ => continue,
        };
        if !dict.has(b"NM") {
            dict.set("NM", fresh());
        }
        if !dict.has(b"P") {
            dict.set("P", Object::Reference(page_id));
        }
    }
    if inline_changed {
        match array_id {
            Some(id) => {
                doc.objects.insert(id, Object::Array(items));
            }
            None => doc
                .get_dictionary_mut(page_id)
                .map_err(structure)?
                .set("Annots", Object::Array(items)),
        }
    }
    Ok(())
}

/// Une annotation retirée de la page (supprimée, caviardée) ne doit pas survivre dans le
/// fichier par un objet qui la cite encore (B20, Lot 9) : son **popup** (dont `/Parent` la
/// vise et qui, lui, peut être hors de la zone caviardée), une **réponse** (`/IRT`), ou
/// l'**arbre de structure** d'un PDF balisé (marqueur `/OBJR`). On retire ces liens ; l'élagage
/// qui suit supprime alors la note, son `/Contents` compris.
///
/// \[Apprentissage\] L'élagage garde tout objet **atteignable** depuis la racine : supprimer
/// une note de `/Annots` ne suffit pas tant qu'un autre objet la référence encore.
fn drop_dangling_annotation_links(doc: &mut Document, pages: &[ObjectId]) -> CoreResult<()> {
    // Annotations encore listées sur une page (par référence).
    let listed = |doc: &Document| -> BTreeSet<ObjectId> {
        pages
            .iter()
            .filter_map(|page| page_annots(doc, *page))
            .flat_map(|(_, items)| items)
            .filter_map(|item| item.as_reference().ok())
            .collect()
    };
    let before = listed(doc);
    // 1. Popups dont le parent n'est plus listé : retirés de `/Annots`.
    for page_id in pages {
        let Some((array_id, items)) = page_annots(doc, *page_id) else {
            continue;
        };
        let orphan = |item: &Object| {
            let dict = match item {
                Object::Reference(id) => doc.get_dictionary(*id).ok(),
                Object::Dictionary(dict) => Some(dict),
                _ => None,
            };
            dict.is_some_and(|d| {
                subtype(d) == b"Popup"
                    && d.get(b"Parent")
                        .and_then(Object::as_reference)
                        .is_ok_and(|parent| !before.contains(&parent))
            })
        };
        let kept: Vec<Object> = items.iter().filter(|i| !orphan(i)).cloned().collect();
        if kept.len() == items.len() {
            continue;
        }
        match array_id {
            Some(id) => {
                doc.objects.insert(id, Object::Array(kept));
            }
            None => doc
                .get_dictionary_mut(*page_id)
                .map_err(structure)?
                .set("Annots", Object::Array(kept)),
        }
    }
    // 2. Annotations qui ne sont plus listées nulle part.
    let alive = listed(doc);
    let dead: BTreeSet<ObjectId> = doc
        .objects
        .iter()
        .filter(|(id, object)| {
            !alive.contains(id)
                && object.as_dict().is_ok_and(|d| {
                    d.has(b"Rect")
                        && (d.get(b"Type").and_then(Object::as_name).ok()
                            == Some(b"Annot".as_slice())
                            || d.has(b"Subtype"))
                })
        })
        .map(|(id, _)| *id)
        .collect();
    if dead.is_empty() {
        return Ok(());
    }
    // 3. Liens `/IRT` et `/OBJR` vers elles : retirés partout, y compris dans les objets directs.
    for object in doc.objects.values_mut() {
        scrub_links(object, &dead, 0);
    }
    Ok(())
}

/// Retire de `object` (et de ce qu'il contient directement) les clés `/IRT` et les `/Obj` de
/// marqueurs `/OBJR` qui visent une annotation de `dead`. Profondeur bornée : un objet direct
/// ne peut pas se contenir lui-même, mais un fichier hostile peut l'imbriquer très loin.
fn scrub_links(object: &mut Object, dead: &BTreeSet<ObjectId>, depth: usize) {
    if depth > 64 {
        return;
    }
    let targets_dead = |o: Option<&Object>| {
        o.and_then(|o| o.as_reference().ok())
            .is_some_and(|id| dead.contains(&id))
    };
    match object {
        Object::Dictionary(dict) => {
            if targets_dead(dict.get(b"IRT").ok()) {
                dict.remove(b"IRT");
            }
            let is_objr =
                dict.get(b"Type").and_then(Object::as_name).ok() == Some(b"OBJR".as_slice());
            if is_objr && targets_dead(dict.get(b"Obj").ok()) {
                dict.remove(b"Obj");
            }
            for (_, value) in dict.iter_mut() {
                scrub_links(value, dead, depth + 1);
            }
        }
        Object::Array(items) => {
            for item in items.iter_mut() {
                scrub_links(item, dead, depth + 1);
            }
        }
        Object::Stream(stream) => {
            for (_, value) in stream.dict.iter_mut() {
                scrub_links(value, dead, depth + 1);
            }
        }
        _ => {}
    }
}

fn subtype(dict: &Dictionary) -> Vec<u8> {
    dict.get(b"Subtype")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(|n| n.to_vec())
        .unwrap_or_default()
}

/// Copie corrigée d'un dictionnaire d'annotation, ou `None` s'il n'y a rien à changer.
///
/// \[Apprentissage\] On renvoie une **copie** plutôt que de modifier sur place : la
/// lecture de l'apparence emprunte `doc` en lecture, et Rust interdit de le modifier en
/// même temps. Calculer d'abord, écrire ensuite, lève le conflit d'emprunts.
fn cleaned(doc: &Document, dict: &Dictionary) -> Option<Dictionary> {
    let mut new_dict = dict.clone();
    let mut changed = false;
    // La clé privée de PDFium n'a rien à faire dans un fichier, quel que soit le sous-type ;
    // l'apparence générée, elle, n'est retirée que des notes (icône jaune imposée) : celle
    // des marquages est portable et attendue par les lecteurs qui ne régénèrent rien.
    if new_dict.has(b"PDFIUM_HasGeneratedAP") {
        new_dict.remove(b"PDFIUM_HasGeneratedAP");
        if subtype(dict).as_slice() == b"Text" {
            new_dict.remove(b"AP");
        }
        changed = true;
    }
    if subtype(dict).as_slice() == b"Ink"
        && !dict.has(b"InkList")
        && let Some(points) = ink_points(doc, dict)
    {
        new_dict.set("InkList", Object::Array(points));
        changed = true;
    }
    changed.then_some(new_dict)
}

/// `/InkList` déduite de l'apparence normale : un tableau de points par sous-chemin.
fn ink_points(doc: &Document, dict: &Dictionary) -> Option<Vec<Object>> {
    let ap = dict.get(b"AP").ok()?;
    let (_, ap) = doc.dereference(ap).ok()?;
    let n = ap.as_dict().ok()?.get(b"N").ok()?;
    let (_, stream) = doc.dereference(n).ok()?;
    let stream = stream.as_stream().ok()?;
    let data = stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone());
    let content = Content::decode(&data).ok()?;
    let mut strokes: Vec<Vec<f32>> = Vec::new();
    let mut current: Vec<f32> = Vec::new();
    let num = |o: &Object| o.as_float().ok();
    // Une courbe de Bézier est échantillonnée en 8 segments depuis le point courant : une
    // `/InkList` réduite aux extrémités ferait d'une ellipse un losange (B25, Lot 9).
    let curve = |current: &mut Vec<f32>, c1: [f32; 2], c2: [f32; 2], end: [f32; 2]| {
        let [Some(&x0), Some(&y0)] = [current.len().checked_sub(2), current.len().checked_sub(1)]
            .map(|i| i.and_then(|i| current.get(i)))
        else {
            current.extend(end);
            return;
        };
        for step in 1..=8 {
            let t = step as f32 / 8.0;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            current.push(a * x0 + b * c1[0] + c * c2[0] + d * end[0]);
            current.push(a * y0 + b * c1[1] + c * c2[1] + d * end[1]);
        }
    };
    for op in content.operations.iter().take(100_000) {
        let v: Vec<f32> = op.operands.iter().filter_map(num).collect();
        match (op.operator.as_str(), v.as_slice()) {
            ("m", [x, y]) => {
                if current.len() >= 4 {
                    strokes.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
                current.extend([*x, *y]);
            }
            ("l", [x, y]) => current.extend([*x, *y]),
            ("c", [x1, y1, x2, y2, x, y]) => curve(&mut current, [*x1, *y1], [*x2, *y2], [*x, *y]),
            ("v", [x2, y2, x, y]) => {
                // Premier point de contrôle = point courant.
                let start = [
                    current
                        .get(current.len().wrapping_sub(2))
                        .copied()
                        .unwrap_or(*x2),
                    current.last().copied().unwrap_or(*y2),
                ];
                curve(&mut current, start, [*x2, *y2], [*x, *y]);
            }
            ("y", [x1, y1, x, y]) => curve(&mut current, [*x1, *y1], [*x, *y], [*x, *y]),
            ("re", [x, y, w, h]) => {
                if current.len() >= 4 {
                    strokes.push(std::mem::take(&mut current));
                }
                strokes.push(vec![*x, *y, x + w, *y, x + w, y + h, *x, y + h, *x, *y]);
                current.clear();
            }
            ("h", _) if current.len() >= 4 => {
                current.extend([current[0], current[1]]);
            }
            _ => {}
        }
    }
    if current.len() >= 4 {
        strokes.push(current);
    }
    if strokes.is_empty() {
        return None;
    }
    // Les points de l'apparence sont dans l'espace du formulaire : `/Matrix` les ramène dans
    // celui de la page (§8.10.1).
    let matrix: Option<Vec<f32>> = stream
        .dict
        .get(b"Matrix")
        .and_then(Object::as_array)
        .ok()
        .map(|m| m.iter().filter_map(num).collect());
    if let Some([a, b, c, d, e, f]) = matrix.as_deref() {
        for stroke in &mut strokes {
            for [x, y] in stroke.as_chunks_mut::<2>().0 {
                (*x, *y) = (a * *x + c * *y + e, b * *x + d * *y + f);
            }
        }
    }
    Some(
        strokes
            .into_iter()
            .map(|s| Object::Array(s.into_iter().map(Object::Real).collect()))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_sanity_rejects_truncated_or_page_losing_output() {
        let bytes = crate::testing::many_pages_pdf(3);
        assert!(rewrite_is_sane(3, &bytes));
        assert!(!rewrite_is_sane(4, &bytes), "nombre de pages différent");
        assert!(
            !rewrite_is_sane(3, &bytes[..bytes.len() / 2]),
            "fichier tronqué"
        );
        assert!(!rewrite_is_sane(0, b"pas un pdf"));
    }
}
