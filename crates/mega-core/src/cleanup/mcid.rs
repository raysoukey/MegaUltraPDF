// SPDX-License-Identifier: GPL-3.0-or-later
//! MCID orphelins de l'arbre de structure (Lot 14, audit E5).
//!
//! Pourquoi : `regenerate_content` (suppression d'un objet) retire du contenu de la page le
//! `BDC /MCID n … EMC` de l'objet supprimé, mais l'arbre de structure d'un PDF balisé
//! (`/StructTreeRoot`) cite toujours ce MCID. Un lecteur d'écran, ou `qpdf --check`, y
//! trouverait une référence vers rien. [`prune_orphan_mcids`] retire ces citations.
//!
//! Limites assumées : seuls les flux de contenu de la page comptent (un MCID placé dans un
//! XObject formulaire n'est pas vu) ; on ne retire que ce qui est prouvé absent d'une page
//! lisible. Un élément de structure écrit en dictionnaire direct dans le `/K` d'un autre
//! n'est pas parcouru, et les nœuds `/Kids` directs du `/ParentTree` non plus.

use std::collections::{BTreeMap, BTreeSet};

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId};

/// Profondeur maximale suivie dans l'arbre de structure et l'arbre de numéros.
const MAX_DEPTH: usize = 64;

/// MCID présents dans le contenu de chaque page lisible (une page illisible est absente de la
/// table : on n'y prune rien).
fn page_mcids(doc: &Document) -> BTreeMap<ObjectId, BTreeSet<i64>> {
    let mut out = BTreeMap::new();
    for page_id in doc.get_pages().into_values() {
        let Ok(bytes) = crate::crypt::page_content(doc, page_id) else {
            continue;
        };
        let Ok(content) = Content::decode(&bytes) else {
            continue;
        };
        let properties = page_properties(doc, page_id);
        let mut found = BTreeSet::new();
        for op in content.operations.iter().filter(|o| o.operator == "BDC") {
            let mcid = match op.operands.get(1) {
                Some(Object::Dictionary(d)) => d.get(b"MCID").ok().and_then(|m| m.as_i64().ok()),
                Some(Object::Name(name)) => properties
                    .get(name)
                    .and_then(|d| d.get(b"MCID").ok())
                    .and_then(|m| m.as_i64().ok()),
                _ => None,
            };
            found.extend(mcid);
        }
        out.insert(page_id, found);
    }
    out
}

/// Dictionnaires de `/Resources /Properties` de la page (nom de propriété -> dictionnaire).
fn page_properties(doc: &Document, page_id: ObjectId) -> BTreeMap<Vec<u8>, Dictionary> {
    let mut out = BTreeMap::new();
    let Ok((resources, ids)) = doc.get_page_resources(page_id) else {
        return out;
    };
    let dicts = resources
        .into_iter()
        .chain(ids.into_iter().filter_map(|id| doc.get_dictionary(id).ok()));
    for resources in dicts {
        let Ok(props) = resources.get(b"Properties") else {
            continue;
        };
        let Ok((_, props)) = doc.dereference(props) else {
            continue;
        };
        let Ok(props) = props.as_dict() else {
            continue;
        };
        for (name, value) in props.iter() {
            if let Ok((_, Object::Dictionary(d))) = doc.dereference(value) {
                out.entry(name.clone()).or_insert_with(|| d.clone());
            }
        }
    }
    out
}

/// Si `item` (un élément du `/K` d'un élément de structure) cite un MCID absent de la page
/// qu'il vise, rend ce couple `(page, mcid)`.
fn orphan_of(
    item: &Object,
    element_page: Option<ObjectId>,
    present: &BTreeMap<ObjectId, BTreeSet<i64>>,
) -> Option<(ObjectId, i64)> {
    let (mcid, page) = match item {
        Object::Integer(n) => (*n, element_page),
        Object::Dictionary(d)
            if d.get(b"Type").and_then(Object::as_name).ok() == Some(b"MCR".as_slice()) =>
        {
            let page = d
                .get(b"Pg")
                .and_then(Object::as_reference)
                .ok()
                .or(element_page);
            (d.get(b"MCID").ok()?.as_i64().ok()?, page)
        }
        _ => return None,
    };
    let page = page?;
    (!present.get(&page)?.contains(&mcid)).then_some((page, mcid))
}

/// Éléments de structure atteignables depuis `/StructTreeRoot` (références seulement), pile
/// explicite, profondeur bornée et ensemble des visités (un fichier peut boucler).
fn struct_elements(doc: &Document, root: ObjectId) -> Vec<ObjectId> {
    let mut seen = BTreeSet::from([root]);
    let mut out = Vec::new();
    let mut stack = vec![(root, 0usize)];
    while let Some((id, depth)) = stack.pop() {
        let Ok(dict) = doc.get_dictionary(id) else {
            continue;
        };
        if id != root {
            out.push(id);
        }
        if depth >= MAX_DEPTH {
            continue;
        }
        let kids: Vec<&Object> = match dict.get(b"K") {
            Ok(Object::Array(items)) => items.iter().collect(),
            Ok(single) => vec![single],
            Err(_) => vec![],
        };
        for kid in kids {
            if let Object::Reference(kid_id) = kid
                && seen.insert(*kid_id)
            {
                stack.push((*kid_id, depth + 1));
            }
        }
    }
    out
}

/// Retire de l'arbre de structure les citations de MCID absents du contenu de leur page, et
/// l'entrée correspondante du `/ParentTree`.
///
/// Un élément dont le `/K` se vide perd aussi `/Alt` et `/ActualText` (ils décrivaient le
/// contenu disparu), comme pour les marqueurs `/OBJR` (`scrub_links`).
///
/// \[Apprentissage\] Les chaînes `if let … && …` et `let … else` évitent les pyramides de
/// `match` imbriqués (voir `docs/apprentissage/`).
pub(crate) fn prune_orphan_mcids(doc: &mut Document) {
    let Some(root) = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"StructTreeRoot").ok())
        .and_then(|r| r.as_reference().ok())
    else {
        return;
    };
    let present = page_mcids(doc);
    let mut removed: BTreeSet<(ObjectId, i64)> = BTreeSet::new();
    for id in struct_elements(doc, root) {
        let Ok(dict) = doc.get_dictionary(id) else {
            continue;
        };
        let page = dict.get(b"Pg").and_then(Object::as_reference).ok();
        let mut new_k = dict.get(b"K").ok().cloned();
        let mut hit = Vec::new();
        let emptied = match &mut new_k {
            Some(Object::Array(items)) => {
                let before = items.len();
                items.retain(|item| match orphan_of(item, page, &present) {
                    Some(pair) => {
                        hit.push(pair);
                        false
                    }
                    None => true,
                });
                before > 0 && items.is_empty()
            }
            Some(single) => match orphan_of(single, page, &present) {
                Some(pair) => {
                    hit.push(pair);
                    true
                }
                None => false,
            },
            None => false,
        };
        if hit.is_empty() {
            continue;
        }
        removed.extend(hit);
        if let Ok(dict) = doc.get_dictionary_mut(id) {
            if emptied {
                dict.remove(b"K");
                dict.remove(b"Alt");
                dict.remove(b"ActualText");
            } else if let Some(k) = new_k {
                dict.set("K", k);
            }
        }
    }
    if !removed.is_empty() {
        null_parent_tree_entries(doc, root, &removed);
    }
}

/// Met à `null` l'entrée `[mcid]` du tableau `/ParentTree` de chaque page concernée (clé =
/// `/StructParents` de la page). Les nœuds de l'arbre de numéros se suivent par `/Kids`.
fn null_parent_tree_entries(
    doc: &mut Document,
    root: ObjectId,
    removed: &BTreeSet<(ObjectId, i64)>,
) {
    // clé /StructParents -> MCID à effacer
    let mut by_key: BTreeMap<i64, BTreeSet<usize>> = BTreeMap::new();
    for (page, mcid) in removed {
        let key = doc
            .get_dictionary(*page)
            .ok()
            .and_then(|p| p.get(b"StructParents").ok())
            .and_then(|k| k.as_i64().ok());
        if let (Some(key), Ok(mcid)) = (key, usize::try_from(*mcid)) {
            by_key.entry(key).or_default().insert(mcid);
        }
    }
    let Some(parent_tree) = doc
        .get_dictionary(root)
        .ok()
        .and_then(|r| r.get(b"ParentTree").ok())
        .and_then(|p| p.as_reference().ok())
    else {
        return;
    };
    let mut seen = BTreeSet::from([parent_tree]);
    let mut stack = vec![(parent_tree, 0usize)];
    // Tableaux indirects à corriger après le parcours (le parcours emprunte `doc`).
    let mut arrays: Vec<(ObjectId, BTreeSet<usize>)> = Vec::new();
    while let Some((id, depth)) = stack.pop() {
        let Ok(node) = doc.get_dictionary_mut(id) else {
            continue;
        };
        if let Ok(Object::Array(nums)) = node.get_mut(b"Nums") {
            for pair in nums.chunks_mut(2) {
                let [key, value] = pair else { continue };
                let Some(indices) = key.as_i64().ok().and_then(|k| by_key.get(&k)) else {
                    continue;
                };
                match value {
                    Object::Array(entries) => null_entries(entries, indices),
                    Object::Reference(array) => arrays.push((*array, indices.clone())),
                    _ => {}
                }
            }
        }
        if depth >= MAX_DEPTH {
            continue;
        }
        if let Ok(Object::Array(kids)) = node.get(b"Kids") {
            for kid in kids {
                if let Object::Reference(kid_id) = kid
                    && seen.insert(*kid_id)
                {
                    stack.push((*kid_id, depth + 1));
                }
            }
        }
    }
    for (id, indices) in arrays {
        if let Ok(Object::Array(entries)) = doc.get_object_mut(id) {
            null_entries(entries, &indices);
        }
    }
}

fn null_entries(entries: &mut [Object], indices: &BTreeSet<usize>) {
    for index in indices {
        if let Some(entry) = entries.get_mut(*index) {
            *entry = Object::Null;
        }
    }
}
