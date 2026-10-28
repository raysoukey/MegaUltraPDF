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
/// orphelins élagués et renumérotés. Un document chiffré (ouvert avec `password`) est nettoyé
/// en clair puis **rechiffré à l'identique** (`crypt::LopdfSession`, Lot 10).
pub fn before_save(bytes: &[u8], password: Option<&str>) -> CoreResult<Vec<u8>> {
    let mut session = match crate::crypt::LopdfSession::load(bytes, password) {
        Ok(session) => session,
        // Un fichier que lopdf ne relit (ou ne déchiffre) pas est écrit tel que PDFium l'a
        // produit, protection comprise.
        Err(_) => return Ok(bytes.to_vec()),
    };
    let doc = &mut session.doc;
    let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
    // Le nettoyage ne touche à aucun flux de contenu : on en garde l'empreinte pour vérifier
    // la relecture (un rechiffrement faux brouillerait le contenu sans changer les pages).
    let contents_before = page_contents(doc);
    for page_id in &pages {
        clean_page_annotations(doc, *page_id)?;
        identify_annotations(doc, *page_id)?;
    }
    drop_dangling_annotation_links(doc, &pages)?;
    dedupe_font_files(doc);
    // Le fichier est réécrit : sa date de modification aussi (C12, Lot 9).
    crate::properties::stamp_mod_date(doc);
    let pages_before = doc.get_pages().len();
    prune(doc);
    doc.renumber_objects();
    let out = session.finish()?;
    // Ceinture de sécurité : la réécriture `lopdf` remplace le fichier de l'utilisateur ; si
    // elle ne se relit (et ne se déchiffre) pas avec le même nombre de pages, on écrit les
    // octets PDFium tels quels.
    if !rewrite_is_sane(pages_before, &out, password) {
        return Ok(bytes.to_vec());
    }
    let reread = crate::crypt::LopdfSession::load(&out, password).map(|s| page_contents(&s.doc));
    if reread.ok().as_ref() != Some(&contents_before) {
        return Ok(bytes.to_vec());
    }
    Ok(out)
}

/// Contenu décodé de chaque page, dans l'ordre des pages.
fn page_contents(doc: &Document) -> Vec<Vec<u8>> {
    doc.get_pages()
        .into_values()
        .map(|id| doc.get_page_content(id))
        .collect()
}

/// Objets atteignables depuis la remorque (racine, `/Info`, `/Encrypt`…), en suivant toutes
/// les références des dictionnaires, tableaux et dictionnaires de flux.
///
/// \[Apprentissage\] `lopdf::Document::prune_objects` tient la liste des objets vus dans
/// un `Vec` et appelle `contains` à chaque référence : temps **quadratique**, sensible sur un
/// gros fichier (C22, Lot 9). Un `BTreeSet` répond à « déjà vu ? » en temps logarithmique.
pub(crate) fn reachable(doc: &Document) -> BTreeSet<ObjectId> {
    fn refs_of(object: &Object, out: &mut Vec<ObjectId>) {
        match object {
            Object::Reference(id) => out.push(*id),
            Object::Array(items) => items.iter().for_each(|o| refs_of(o, out)),
            Object::Dictionary(dict) => dict.iter().for_each(|(_, o)| refs_of(o, out)),
            Object::Stream(stream) => stream.dict.iter().for_each(|(_, o)| refs_of(o, out)),
            _ => {}
        }
    }
    let mut seen = BTreeSet::new();
    let mut todo = Vec::new();
    for (_, value) in doc.trailer.iter() {
        refs_of(value, &mut todo);
    }
    while let Some(id) = todo.pop() {
        if seen.insert(id)
            && let Some(object) = doc.objects.get(&id)
        {
            refs_of(object, &mut todo);
        }
    }
    seen
}

/// Fait pointer les descripteurs de police vers **un seul** exemplaire de chaque programme
/// de police identique (`/FontFile`, `/FontFile2`, `/FontFile3`) ; l'élagage retire ensuite
/// les copies. Rend le nombre de références réécrites.
///
/// PDFium incorpore la police Unicode **entière** à chaque chargement (Lot 10) : après un
/// rechargement du document (filigrane, annuler…), une nouvelle insertion de texte en
/// recharge une copie. Sans cette passe, chaque copie ajouterait environ 400 Ko au fichier.
pub(crate) fn dedupe_font_files(doc: &mut Document) -> usize {
    use std::collections::BTreeMap;
    const KEYS: [&[u8]; 3] = [b"FontFile", b"FontFile2", b"FontFile3"];
    // Programme de police (filtre + octets) → premier objet qui le porte.
    let mut first: BTreeMap<(Vec<u8>, Vec<u8>), ObjectId> = BTreeMap::new();
    let mut rewrites: Vec<(ObjectId, &'static [u8], ObjectId)> = Vec::new();
    for (id, object) in &doc.objects {
        let Ok(dict) = object.as_dict() else {
            continue;
        };
        if dict.get(b"Type").ok() != Some(&Object::Name(b"FontDescriptor".to_vec())) {
            continue;
        }
        for key in KEYS {
            let Ok(file) = dict.get(key).and_then(Object::as_reference) else {
                continue;
            };
            let Some(Ok(stream)) = doc.objects.get(&file).map(Object::as_stream) else {
                continue;
            };
            let filter = stream
                .dict
                .get(b"Filter")
                .ok()
                .map(|f| format!("{f:?}").into_bytes())
                .unwrap_or_default();
            let canonical = *first
                .entry((filter, stream.content.clone()))
                .or_insert(file);
            if canonical != file {
                rewrites.push((*id, key, canonical));
            }
        }
    }
    for (descriptor, key, canonical) in &rewrites {
        if let Ok(dict) = doc.get_dictionary_mut(*descriptor) {
            dict.set(*key, Object::Reference(*canonical));
        }
    }
    rewrites.len()
}

/// Retire les objets inatteignables (orphelins) et rend leur nombre.
pub(crate) fn prune(doc: &mut Document) -> usize {
    let keep = reachable(doc);
    let before = doc.objects.len();
    doc.objects.retain(|id, _| keep.contains(id));
    before - doc.objects.len()
}

/// Vrai si `out` se relit (et, s'il est chiffré, se déchiffre avec `password`) comme un PDF
/// non vide ayant encore `pages_before` pages.
pub(crate) fn rewrite_is_sane(pages_before: usize, out: &[u8], password: Option<&str>) -> bool {
    match crate::crypt::LopdfSession::load(out, password) {
        Ok(reloaded) => {
            reloaded.doc.get_pages().len() == pages_before
                && reloaded.doc.trailer.get(b"Root").is_ok()
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
    // 0. Membres d'un groupe (`/IRT` + `/RT /Group`, §12.5.6.2) dont l'annotation principale
    //    n'est plus listée : le groupe forme une seule annotation, ils partent avec elle.
    let primaries = listed(doc);
    for page_id in pages {
        let Some((array_id, items)) = page_annots(doc, *page_id) else {
            continue;
        };
        let orphan_member = |item: &Object| {
            let dict = match item {
                Object::Reference(id) => doc.get_dictionary(*id).ok(),
                Object::Dictionary(dict) => Some(dict),
                _ => None,
            };
            dict.is_some_and(|d| {
                d.get(b"RT").and_then(Object::as_name).ok() == Some(b"Group".as_slice())
                    && d.get(b"IRT")
                        .and_then(Object::as_reference)
                        .is_ok_and(|primary| !primaries.contains(&primary))
            })
        };
        let kept: Vec<Object> = items
            .iter()
            .filter(|i| !orphan_member(i))
            .cloned()
            .collect();
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
    // 3. Leur texte d'abord, par précaution : même si un lien oublié les garde atteignables,
    //    plus rien de lisible n'y reste (relecture du Lot 9).
    for id in &dead {
        if let Ok(dict) = doc.get_dictionary_mut(*id) {
            for key in [
                b"Contents".as_slice(),
                b"RC",
                b"T",
                b"Subj",
                b"Popup",
                b"AP",
                b"FS",
            ] {
                dict.remove(key);
            }
        }
    }
    // 4. Marqueurs `/OBJR` vers elles rangés en objets indirects.
    let dead_objr: BTreeSet<ObjectId> = doc
        .objects
        .iter()
        .filter(|(_, o)| o.as_dict().is_ok_and(|d| is_objr_to(d, &dead)))
        .map(|(id, _)| *id)
        .collect();
    // 5. Liens `/IRT` et entrées `/OBJR` vers elles : retirés partout, objets directs compris.
    for object in doc.objects.values_mut() {
        scrub_links(object, &dead, &dead_objr, 0);
    }
    Ok(())
}

/// Vrai si `dict` est un marqueur de contenu `/OBJR` dont la cible `/Obj` est dans `dead`.
fn is_objr_to(dict: &Dictionary, dead: &BTreeSet<ObjectId>) -> bool {
    dict.get(b"Type").and_then(Object::as_name).ok() == Some(b"OBJR".as_slice())
        && dict
            .get(b"Obj")
            .and_then(Object::as_reference)
            .is_ok_and(|id| dead.contains(&id))
}

/// Retire de `object` (et de ce qu'il contient directement) les clés `/IRT` qui visent une
/// annotation de `dead`, et les entrées `/OBJR` qui la citent dans le `/K` d'un élément de
/// structure : le marqueur entier part (sa clé `/Obj` est obligatoire, §14.7.4.3), et avec
/// lui le texte qui décrivait l'annotation (`/Alt`, `/ActualText`, `/E`). Profondeur bornée :
/// un objet direct ne peut pas se contenir lui-même, mais un fichier hostile peut
/// l'imbriquer très loin.
fn scrub_links(
    object: &mut Object,
    dead: &BTreeSet<ObjectId>,
    dead_objr: &BTreeSet<ObjectId>,
    depth: usize,
) {
    if depth > 64 {
        return;
    }
    let is_dead_marker = |o: &Object| match o {
        Object::Dictionary(d) => is_objr_to(d, dead),
        Object::Reference(id) => dead_objr.contains(id),
        _ => false,
    };
    match object {
        Object::Dictionary(dict) => {
            if dict
                .get(b"IRT")
                .and_then(Object::as_reference)
                .is_ok_and(|id| dead.contains(&id))
            {
                dict.remove(b"IRT");
            }
            let mut removed = false;
            let mut emptied = false;
            match dict.get_mut(b"K") {
                Ok(Object::Array(kids)) => {
                    let before = kids.len();
                    kids.retain(|k| !is_dead_marker(k));
                    removed = kids.len() != before;
                    emptied = kids.is_empty();
                }
                Ok(k) if is_dead_marker(k) => {
                    removed = true;
                    emptied = true;
                }
                _ => {}
            }
            if emptied {
                dict.remove(b"K");
            }
            if removed {
                for key in [b"Alt".as_slice(), b"ActualText", b"E"] {
                    dict.remove(key);
                }
            }
            for (_, value) in dict.iter_mut() {
                scrub_links(value, dead, dead_objr, depth + 1);
            }
        }
        Object::Array(items) => {
            for item in items.iter_mut() {
                scrub_links(item, dead, dead_objr, depth + 1);
            }
        }
        Object::Stream(stream) => {
            for (_, value) in stream.dict.iter_mut() {
                scrub_links(value, dead, dead_objr, depth + 1);
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
        assert!(rewrite_is_sane(3, &bytes, None));
        assert!(
            !rewrite_is_sane(4, &bytes, None),
            "nombre de pages différent"
        );
        assert!(
            !rewrite_is_sane(3, &bytes[..bytes.len() / 2], None),
            "fichier tronqué"
        );
        assert!(!rewrite_is_sane(0, b"pas un pdf", None));
    }
}

#[cfg(test)]
mod prune_tests {
    use super::*;

    /// Même résultat que `lopdf`, y compris sur des références cycliques (signets en boucle,
    /// popup et note qui se citent l'un l'autre).
    #[test]
    fn prune_matches_lopdf_on_cycles() {
        for bytes in [
            crate::testing::cyclic_outline_pdf(),
            crate::testing::note_with_popup_pdf("x"),
            crate::testing::heavy_images_pdf(),
        ] {
            let mut ours = Document::load_mem(&bytes).unwrap();
            let orphan = ours.add_object(Object::Integer(42));
            let mut theirs = ours.clone();
            assert!(prune(&mut ours) >= 1);
            theirs.prune_objects();
            let keys = |d: &Document| d.objects.keys().copied().collect::<Vec<_>>();
            assert_eq!(keys(&ours), keys(&theirs));
            assert!(!ours.objects.contains_key(&orphan));
        }
    }
}
