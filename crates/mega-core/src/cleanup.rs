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
    for page_id in pages {
        clean_page_annotations(&mut doc, page_id)?;
    }
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
            ("c", [.., x, y]) => current.extend([*x, *y]),
            ("v", [.., x, y]) | ("y", [.., x, y]) => current.extend([*x, *y]),
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
