// SPDX-License-Identifier: GPL-3.0-or-later
//! MCID orphelins de l'arbre de structure (Lot 14, audit E5).
//!
//! Pourquoi : `regenerate_content` (suppression d'un objet) retire du contenu de la page le
//! `BDC /MCID n … EMC` de l'objet supprimé, mais l'arbre de structure d'un PDF balisé
//! (`/StructTreeRoot`) cite toujours ce MCID. Un lecteur d'écran, ou `qpdf --check`, y
//! trouverait une référence vers rien. [`prune_orphan_mcids`] retire ces citations.
//!
//! Règle conservatrice : un contenu balisé peut vivre dans un XObject formulaire. On ne retire
//! donc rien (1) pour un `/MCR` qui porte `/Stm` (flux de contenu autre que celui de la page),
//! (2) pour une page dont le contenu décodé dessine par `Do` un XObject **formulaire** (ou un
//! XObject introuvable dans ses ressources, qui pourrait en être un) ; une image (`/Subtype
//! /Image`) ne porte pas de contenu balisé et laisse la page concluante ; ni (3) pour une page
//! illisible. On ne retire que ce qui est prouvé absent.
//! Un élément de structure écrit en dictionnaire direct dans le `/K` d'un autre n'est pas
//! parcouru, et les nœuds `/Kids` directs du `/ParentTree` non plus.

use std::collections::{BTreeMap, BTreeSet};

use lopdf::content::Content;
use lopdf::{Document, Object, ObjectId};

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
        if draws_a_form(doc, page_id, &content) {
            continue;
        }
        let properties = page_resources(doc, page_id, b"Properties");
        let mut found = BTreeSet::new();
        for op in content.operations.iter().filter(|o| o.operator == "BDC") {
            let mcid = match op.operands.get(1) {
                Some(Object::Dictionary(d)) => d.get(b"MCID").ok().and_then(|m| m.as_i64().ok()),
                Some(Object::Name(name)) => properties
                    .get(name)
                    .and_then(|o| o.as_dict().ok())
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

/// Vrai si le contenu de la page dessine par `Do` un XObject qui peut porter des MCID : un
/// formulaire, ou un nom introuvable dans les ressources (prudence). Une image n'en porte pas.
fn draws_a_form(doc: &Document, page_id: ObjectId, content: &Content) -> bool {
    let mut calls = content.operations.iter().filter(|o| o.operator == "Do");
    let Some(first) = calls.next() else {
        return false;
    };
    let xobjects = page_resources(doc, page_id, b"XObject");
    let is_image = |name: &Object| {
        name.as_name()
            .ok()
            .and_then(|name| xobjects.get(name))
            .and_then(|x| x.as_stream().ok())
            .is_some_and(|x| {
                x.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image".as_slice())
            })
    };
    std::iter::once(first)
        .chain(calls)
        .any(|op| !op.operands.first().is_some_and(is_image))
}

/// Entrées de la catégorie `category` (`/Properties`, `/XObject`) des ressources de la page,
/// héritées comprises : nom -> objet (référence suivie). La définition la plus proche de la
/// page l'emporte.
fn page_resources<'a>(
    doc: &'a Document,
    page_id: ObjectId,
    category: &[u8],
) -> BTreeMap<Vec<u8>, &'a Object> {
    let mut out = BTreeMap::new();
    let Ok((resources, ids)) = doc.get_page_resources(page_id) else {
        return out;
    };
    let dicts = resources
        .into_iter()
        .chain(ids.into_iter().filter_map(|id| doc.get_dictionary(id).ok()));
    for resources in dicts {
        let Ok(entries) = resources.get(category) else {
            continue;
        };
        let Ok((_, entries)) = doc.dereference(entries) else {
            continue;
        };
        let Ok(entries) = entries.as_dict() else {
            continue;
        };
        for (name, value) in entries.iter() {
            if let Ok((_, object)) = doc.dereference(value) {
                out.entry(name.clone()).or_insert(object);
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
        // `/Stm` : le MCID vit dans un autre flux (XObject), invisible d'ici.
        Object::Dictionary(d) if d.has(b"Stm") => return None,
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
/// \[Apprentissage\] Parcours à pile explicite plutôt que récursif, avec ensemble des visités :
/// voir `docs/apprentissage/lot-13.md`.
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

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Dictionary, Stream, dictionary};

    /// Document balisé : une page par contenu, un `/StructTreeRoot` sans enfant.
    fn tagged(contents: &[&str], props: Option<Dictionary>) -> (Document, Vec<ObjectId>, ObjectId) {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let root = doc.new_object_id();
        let mut pages = Vec::new();
        for content in contents {
            let stream = doc.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
            let mut resources = Dictionary::new();
            if let Some(props) = &props {
                resources.set("Properties", props.clone());
            }
            pages.push(doc.add_object(dictionary! {
                "Type" => "Page", "Parent" => pages_id, "Contents" => stream,
                "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
                "Resources" => resources,
            }));
        }
        let kids: Vec<Object> = pages.iter().map(|p| (*p).into()).collect();
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => kids, "Count" => pages.len() as i64,
            }),
        );
        doc.objects.insert(
            root,
            Object::Dictionary(
                dictionary! { "Type" => "StructTreeRoot", "K" => Vec::<Object>::new() },
            ),
        );
        let catalog = doc.add_object(dictionary! {
            "Type" => "Catalog", "Pages" => pages_id, "StructTreeRoot" => root,
        });
        doc.trailer.set("Root", catalog);
        (doc, pages, root)
    }

    fn elem(doc: &mut Document, root: ObjectId, page: ObjectId, k: Object) -> ObjectId {
        let id = doc.add_object(dictionary! {
            "Type" => "StructElem", "S" => "P", "P" => root, "Pg" => page,
            "K" => k, "Alt" => Object::string_literal("texte"),
        });
        if let Ok(r) = doc.get_dictionary_mut(root)
            && let Ok(Object::Array(kids)) = r.get_mut(b"K")
        {
            kids.push(id.into());
        }
        id
    }

    fn mcr(mcid: i64, page: Option<ObjectId>) -> Object {
        let mut d = dictionary! { "Type" => "MCR", "MCID" => mcid };
        if let Some(p) = page {
            d.set("Pg", p);
        }
        Object::Dictionary(d)
    }

    fn k_of(doc: &Document, id: ObjectId) -> Option<Object> {
        doc.get_dictionary(id).unwrap().get(b"K").ok().cloned()
    }

    #[test]
    fn array_k_keeps_present_integers_and_mcrs_and_drops_orphans() {
        let (mut doc, pages, root) = tagged(&["/P <</MCID 0>> BDC EMC"], None);
        let e = elem(
            &mut doc,
            root,
            pages[0],
            Object::Array(vec![0.into(), 1.into(), mcr(0, None), mcr(2, None)]),
        );
        prune_orphan_mcids(&mut doc);
        assert_eq!(
            k_of(&doc, e),
            Some(Object::Array(vec![0.into(), mcr(0, None)]))
        );
        assert!(doc.get_dictionary(e).unwrap().has(b"Alt"));
    }

    #[test]
    fn an_emptied_k_loses_alt_and_actual_text() {
        let (mut doc, pages, root) = tagged(&["/P <</MCID 0>> BDC EMC"], None);
        let e = elem(&mut doc, root, pages[0], Object::Array(vec![5.into()]));
        doc.get_dictionary_mut(e)
            .unwrap()
            .set("ActualText", Object::string_literal("x"));
        prune_orphan_mcids(&mut doc);
        let d = doc.get_dictionary(e).unwrap();
        assert!(!d.has(b"K") && !d.has(b"Alt") && !d.has(b"ActualText"));
    }

    #[test]
    fn an_mcr_is_judged_on_its_own_page() {
        let (mut doc, pages, root) = tagged(&["/P <</MCID 0>> BDC EMC", "q Q"], None);
        // Présent sur la page 0, absent de la page 1.
        let on_other = elem(
            &mut doc,
            root,
            pages[0],
            Object::Array(vec![mcr(0, Some(pages[1]))]),
        );
        let on_own = elem(
            &mut doc,
            root,
            pages[1],
            Object::Array(vec![mcr(0, Some(pages[0]))]),
        );
        prune_orphan_mcids(&mut doc);
        assert_eq!(k_of(&doc, on_other), None);
        assert_eq!(
            k_of(&doc, on_own),
            Some(Object::Array(vec![mcr(0, Some(pages[0]))]))
        );
    }

    #[test]
    fn an_mcid_cited_by_two_elements_is_kept_in_both_when_present() {
        let (mut doc, pages, root) = tagged(&["/P <</MCID 0>> BDC EMC"], None);
        let a = elem(&mut doc, root, pages[0], 0.into());
        let b = elem(&mut doc, root, pages[0], 0.into());
        prune_orphan_mcids(&mut doc);
        assert_eq!(k_of(&doc, a), Some(0.into()));
        assert_eq!(k_of(&doc, b), Some(0.into()));
    }

    #[test]
    fn a_bdc_name_is_resolved_through_resources_properties() {
        let props = dictionary! { "MC0" => dictionary! { "MCID" => 3 } };
        let (mut doc, pages, root) = tagged(&["/P /MC0 BDC EMC"], Some(props));
        let kept = elem(&mut doc, root, pages[0], 3.into());
        let gone = elem(&mut doc, root, pages[0], 4.into());
        prune_orphan_mcids(&mut doc);
        assert_eq!(k_of(&doc, kept), Some(3.into()));
        assert_eq!(k_of(&doc, gone), None);
    }

    #[test]
    fn a_cycle_between_elements_terminates() {
        let (mut doc, pages, root) = tagged(&["q Q"], None);
        let a = elem(&mut doc, root, pages[0], 9.into());
        let b = elem(&mut doc, root, pages[0], 9.into());
        doc.get_dictionary_mut(a)
            .unwrap()
            .set("K", vec![Object::from(b), 9.into()]);
        doc.get_dictionary_mut(b)
            .unwrap()
            .set("K", vec![Object::from(a), 9.into()]);
        prune_orphan_mcids(&mut doc);
        assert_eq!(k_of(&doc, a), Some(Object::Array(vec![b.into()])));
    }

    #[test]
    fn a_document_without_struct_tree_is_untouched() {
        let (mut doc, _, _) = tagged(&["q Q"], None);
        let catalog = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        doc.get_dictionary_mut(catalog)
            .unwrap()
            .remove(b"StructTreeRoot");
        let before = doc.objects.clone();
        prune_orphan_mcids(&mut doc);
        assert_eq!(doc.objects, before);
    }

    /// Ajoute à la page `page` un XObject `name` de sous-type `subtype`.
    fn add_xobject(doc: &mut Document, page: ObjectId, name: &str, subtype: &str) {
        let xobject = doc.add_object(Stream::new(
            dictionary! { "Type" => "XObject", "Subtype" => subtype },
            Vec::new(),
        ));
        let page = doc.get_dictionary_mut(page).unwrap();
        let mut resources = page.get(b"Resources").unwrap().as_dict().unwrap().clone();
        resources.set("XObject", dictionary! { name => xobject });
        page.set("Resources", resources);
    }

    /// Revue du Lot 14 : une image appelée par `Do` ne porte pas de contenu balisé ; la page
    /// reste concluante et l'orphelin est retiré.
    #[test]
    fn a_page_drawing_an_image_is_pruned_normally() {
        let (mut doc, pages, root) = tagged(&["/P <</MCID 0>> BDC EMC /Im0 Do"], None);
        add_xobject(&mut doc, pages[0], "Im0", "Image");
        let e = elem(&mut doc, root, pages[0], 1.into());
        prune_orphan_mcids(&mut doc);
        assert_eq!(k_of(&doc, e), None);
    }

    /// Un XObject formulaire peut porter des MCID de la page : page non concluante.
    #[test]
    fn a_page_drawing_a_form_xobject_is_never_pruned() {
        let (mut doc, pages, root) = tagged(&["/P <</MCID 0>> BDC EMC /Fm0 Do"], None);
        add_xobject(&mut doc, pages[0], "Fm0", "Form");
        let e = elem(&mut doc, root, pages[0], 1.into());
        prune_orphan_mcids(&mut doc);
        assert_eq!(k_of(&doc, e), Some(1.into()));
    }

    /// Un XObject introuvable dans les ressources pourrait être un formulaire : prudence.
    #[test]
    fn a_page_drawing_an_unknown_xobject_is_never_pruned() {
        let (mut doc, pages, root) = tagged(&["/P <</MCID 0>> BDC EMC /Fm0 Do"], None);
        let e = elem(&mut doc, root, pages[0], 1.into());
        prune_orphan_mcids(&mut doc);
        assert_eq!(k_of(&doc, e), Some(1.into()));
    }

    #[test]
    fn an_mcr_with_stm_is_kept() {
        let (mut doc, pages, root) = tagged(&["q Q"], None);
        let mut m = dictionary! { "Type" => "MCR", "MCID" => 7 };
        m.set("Stm", Object::Reference((99, 0)));
        let e = elem(
            &mut doc,
            root,
            pages[0],
            Object::Array(vec![Object::Dictionary(m.clone())]),
        );
        prune_orphan_mcids(&mut doc);
        assert_eq!(
            k_of(&doc, e),
            Some(Object::Array(vec![Object::Dictionary(m)]))
        );
    }
}
