// SPDX-License-Identifier: GPL-3.0-or-later
//! Destinations : destinations nommées, signets et liens des pages copiées (audit E3).
//!
//! Une destination désigne une page **par sa référence d'objet** (`[12 0 R /Fit]`) ou par un
//! nom qui mène à un tel tableau. Après la copie, seules comptent celles dont la page a été
//! copiée : elles sont réécrites vers la copie, les autres abandonnées.

use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Document as LoDocument, Object, ObjectId};

use super::{MAX_DEPTH, PageMap, catalog_dict, clone_object, name_tree, set_name_tree};

/// Destinations nommées d'un document : nom (octets) → valeur telle qu'écrite (tableau,
/// `<< /D [...] >>` ou référence vers l'un d'eux).
pub(crate) type Named = HashMap<Vec<u8>, Object>;

/// Les destinations nommées de `src` : arbre `/Names /Dests` (PDF 1.2) et dictionnaire
/// `/Dests` du catalogue (PDF 1.1) ; l'arbre l'emporte sur un nom présent dans les deux.
pub(crate) fn named_destinations(src: &LoDocument) -> Named {
    let mut named = Named::new();
    if let Some(dests) = catalog_dict(src, b"Dests") {
        for (name, value) in dests.iter() {
            named.insert(name.clone(), value.clone());
        }
    }
    for (key, value) in name_tree(src, b"Dests") {
        if let Ok(name) = key.as_str() {
            named.insert(name.to_vec(), value);
        }
    }
    named
}

/// Le tableau de destination explicite `[page …]` que désigne `dest` : un tableau, ou le nom
/// (chaîne ou nom PDF) d'une destination nommée, éventuellement sous la forme `<< /D [...] >>`.
fn resolve<'a>(src: &'a LoDocument, dest: &'a Object, named: &'a Named) -> Option<&'a [Object]> {
    let (_, dest) = src.dereference(dest).ok()?;
    let dest = match dest {
        Object::String(name, _) | Object::Name(name) => {
            src.dereference(named.get(name.as_slice())?).ok()?.1
        }
        other => other,
    };
    let dest = match dest {
        Object::Dictionary(dict) => src.dereference(dict.get(b"D").ok()?).ok()?.1,
        other => other,
    };
    dest.as_array().ok().map(Vec::as_slice)
}

/// La destination d'un signet ou d'un lien : sa `/Dest`, ou la `/D` de son action `/GoTo`.
/// `None` pour un autre type d'action (lien web, script…).
fn goto_dest<'a>(src: &'a LoDocument, item: &'a Dictionary) -> Option<&'a Object> {
    if let Ok(dest) = item.get(b"Dest") {
        return Some(dest);
    }
    let (_, action) = src.dereference(item.get(b"A").ok()?).ok()?;
    let action = action.as_dict().ok()?;
    if action.get(b"S").and_then(Object::as_name).ok()? != b"GoTo" {
        return None;
    }
    action.get(b"D").ok()
}

/// Copie explicite (`[page_copiée /Fit …]`) de la destination `dest`, si elle mène à une page
/// copiée ; `None` sinon.
fn carried_explicit(
    src: &LoDocument,
    dst: &mut LoDocument,
    dest: &Object,
    map: &PageMap,
    cache: &mut HashMap<ObjectId, ObjectId>,
    named: &Named,
) -> Option<Object> {
    let explicit = resolve(src, dest, named)?;
    let page = explicit.first()?.as_reference().ok()?;
    map.target(page)?;
    Some(clone_object(
        src,
        dst,
        &Object::Array(explicit.to_vec()),
        map,
        cache,
    ))
}

/// Destination du lien `from` (source) reposée sur sa copie `to` : PDFium retire une
/// destination explicite vers une page qu'il n'a pas (encore) copiée, et un nom pourrait
/// désigner, après une fusion, la page d'une autre source. La destination devient donc
/// explicite vers la page copiée ; un lien vers une page non copiée la perd.
pub(super) fn carry_link_target(
    src: &LoDocument,
    dst: &mut LoDocument,
    map: &PageMap,
    cache: &mut HashMap<ObjectId, ObjectId>,
    named: &Named,
    from: ObjectId,
    to: ObjectId,
) {
    let Ok(link) = src.get_dictionary(from) else {
        return;
    };
    let Some(dest) = goto_dest(src, link) else {
        return;
    };
    let carried = carried_explicit(src, dst, dest, map, cache, named);
    let Ok(copy) = dst.get_dictionary_mut(to) else {
        return;
    };
    copy.remove(b"A");
    copy.remove(b"Dest");
    if let Some(dest) = carried {
        copy.set("Dest", dest);
    }
}

/// Copie d'une destination nommée (`value` : tableau ou `<< /D [...] >>`) si sa page a été
/// copiée.
fn carried_named(
    src: &LoDocument,
    dst: &mut LoDocument,
    value: &Object,
    map: &PageMap,
    cache: &mut HashMap<ObjectId, ObjectId>,
) -> Option<Object> {
    let (_, value) = src.dereference(value).ok()?;
    let explicit = match value {
        Object::Dictionary(dict) => src.dereference(dict.get(b"D").ok()?).ok()?.1,
        other => other,
    };
    let page = explicit.as_array().ok()?.first()?.as_reference().ok()?;
    map.target(page)?;
    Some(clone_object(src, dst, value, map, cache))
}

/// Reporte les destinations nommées (`/Names /Dests` et `/Dests`) dont la page a été copiée,
/// réécrites vers la copie. Lors d'une fusion, un nom déjà présent garde sa première
/// destination (les liens et signets copiés, eux, sont devenus explicites).
pub(crate) fn carry_named_dests(
    src: &LoDocument,
    dst: &mut LoDocument,
    map: &PageMap,
    cache: &mut HashMap<ObjectId, ObjectId>,
) {
    // Arbre de noms (PDF 1.2).
    let tree = name_tree(src, b"Dests");
    if !tree.is_empty() {
        let mut entries = name_tree(dst, b"Dests");
        let mut taken: HashSet<Vec<u8>> = entries
            .iter()
            .filter_map(|(key, _)| key.as_str().ok().map(<[u8]>::to_vec))
            .collect();
        let before = entries.len();
        for (key, value) in tree {
            let Ok(name) = key.as_str() else {
                continue;
            };
            if taken.contains(name) {
                continue;
            }
            if let Some(copy) = carried_named(src, dst, &value, map, cache) {
                taken.insert(name.to_vec());
                entries.push((key, copy));
            }
        }
        if entries.len() > before {
            set_name_tree(dst, b"Dests", entries);
        }
    }
    // Dictionnaire du catalogue (PDF 1.1).
    let Some(dests) = catalog_dict(src, b"Dests") else {
        return;
    };
    let mut kept = catalog_dict(dst, b"Dests").cloned().unwrap_or_default();
    let before = kept.len();
    for (name, value) in dests.iter() {
        if kept.has(name) {
            continue;
        }
        if let Some(copy) = carried_named(src, dst, value, map, cache) {
            kept.set(name.clone(), copy);
        }
    }
    if kept.len() > before
        && let Ok(catalog) = dst.catalog_mut()
    {
        catalog.set("Dests", kept);
    }
}

/// Un signet de la source, lu dans l'arbre `/Outlines`.
struct Item {
    /// Son dictionnaire dans la source (`None` pour la racine).
    id: Option<ObjectId>,
    /// Ses enfants, dans l'ordre (index dans la liste des signets).
    children: Vec<usize>,
    /// Ouvert (`/Count` positif ou absent) ou replié.
    open: bool,
    /// Sa cible dans la copie : (`Dest` ou `A`, valeur), si elle y existe.
    target: Option<(&'static str, Object)>,
}

/// Cible copiée d'un signet : destination explicite vers une page copiée, ou lien web.
fn item_target(
    src: &LoDocument,
    dst: &mut LoDocument,
    item: &Dictionary,
    map: &PageMap,
    cache: &mut HashMap<ObjectId, ObjectId>,
    named: &Named,
) -> Option<(&'static str, Object)> {
    if let Some(dest) = goto_dest(src, item) {
        let dest = carried_explicit(src, dst, dest, map, cache, named)?;
        return Some(("Dest", dest));
    }
    // Un lien web ne dépend d'aucune page : il suit le signet (sans action enchaînée).
    let (_, action) = src.dereference(item.get(b"A").ok()?).ok()?;
    let action = action.as_dict().ok()?;
    if action.get(b"S").and_then(Object::as_name).ok()? != b"URI" {
        return None;
    }
    let mut action = action.clone();
    action.remove(b"Next");
    let action = clone_object(src, dst, &Object::Dictionary(action), map, cache);
    Some(("A", action))
}

/// Les signets de `src`, à plat : la racine en 0, chaque enfant après son parent. Parcours à
/// pile explicite, chaque signet visité une fois, profondeur bornée.
fn read_outline(
    src: &LoDocument,
    dst: &mut LoDocument,
    map: &PageMap,
    cache: &mut HashMap<ObjectId, ObjectId>,
) -> Vec<Item> {
    let named = named_destinations(src);
    let mut items = vec![Item {
        id: None,
        children: Vec::new(),
        open: true,
        target: None,
    }];
    let Some(root) = catalog_dict(src, b"Outlines") else {
        return items;
    };
    let mut seen = HashSet::new();
    // (parent, premier enfant, profondeur)
    let mut stack: Vec<(usize, ObjectId, usize)> = Vec::new();
    if let Ok(first) = root.get(b"First").and_then(Object::as_reference) {
        stack.push((0, first, 1));
    }
    while let Some((parent, first, depth)) = stack.pop() {
        let mut next = Some(first);
        while let Some(id) = next {
            if !seen.insert(id) {
                break;
            }
            let Ok(dict) = src.get_dictionary(id) else {
                break;
            };
            let index = items.len();
            let target = item_target(src, dst, dict, map, cache, &named);
            let open = dict
                .get(b"Count")
                .and_then(Object::as_i64)
                .map_or(true, |c| c >= 0);
            items.push(Item {
                id: Some(id),
                children: Vec::new(),
                open,
                target,
            });
            items[parent].children.push(index);
            if depth < MAX_DEPTH
                && let Ok(child) = dict.get(b"First").and_then(Object::as_reference)
            {
                stack.push((index, child, depth + 1));
            }
            next = dict.get(b"Next").and_then(Object::as_reference).ok();
        }
    }
    items
}

/// Reporte les signets dont la cible a été copiée ; un signet sans cible copiée ne reste que
/// s'il a encore des enfants. Les signets gardés sont renumérotés (`/Parent`, `/Prev`,
/// `/Next`, `/First`, `/Last`) et leurs `/Count` recalculés (§12.3.3). Lors d'une fusion, ils
/// suivent ceux des sources précédentes.
pub(crate) fn carry_outlines(
    src: &LoDocument,
    dst: &mut LoDocument,
    map: &PageMap,
    cache: &mut HashMap<ObjectId, ObjectId>,
) {
    let items = read_outline(src, dst, map, cache);
    // Chaque enfant a un index plus grand que son parent : en remontant la liste, on traite
    // les enfants avant leur parent.
    let mut kept = vec![false; items.len()];
    let mut visible = vec![0i64; items.len()];
    for i in (1..items.len()).rev() {
        let item = &items[i];
        kept[i] = item.target.is_some() || item.children.iter().any(|&c| kept[c]);
    }
    for i in (0..items.len()).rev() {
        visible[i] = items[i]
            .children
            .iter()
            .filter(|&&c| kept[c])
            .map(|&c| 1 + if items[c].open { visible[c] } else { 0 })
            .sum();
    }
    let top: Vec<usize> = items[0]
        .children
        .iter()
        .copied()
        .filter(|&c| kept[c])
        .collect();
    if top.is_empty() {
        return;
    }
    // Racine : celle d'une source précédente (fusion) ou une nouvelle.
    let existing = dst
        .catalog()
        .ok()
        .and_then(|c| c.get(b"Outlines").and_then(Object::as_reference).ok());
    let root_id = existing.unwrap_or_else(|| dst.new_object_id());
    let ids: Vec<Option<ObjectId>> = (0..items.len())
        .map(|i| match i {
            0 => Some(root_id),
            _ if kept[i] => Some(dst.new_object_id()),
            _ => None,
        })
        .collect();
    for (i, item) in items.iter().enumerate().skip(1) {
        let (Some(id), Some(source)) = (ids[i], item.id) else {
            continue;
        };
        let Ok(original) = src.get_dictionary(source) else {
            continue;
        };
        let mut dict = Dictionary::new();
        for key in [&b"Title"[..], b"C", b"F"] {
            if let Ok(value) = original.get(key) {
                let value = clone_object(src, dst, value, map, cache);
                dict.set(key.to_vec(), value);
            }
        }
        if let Some((key, value)) = &item.target {
            dict.set(*key, value.clone());
        }
        let children: Vec<ObjectId> = item
            .children
            .iter()
            .filter_map(|&c| if kept[c] { ids[c] } else { None })
            .collect();
        if let (Some(first), Some(last)) = (children.first(), children.last()) {
            dict.set("First", *first);
            dict.set("Last", *last);
            let count = visible[i];
            dict.set("Count", if item.open { count } else { -count });
        }
        dst.objects.insert(id, Object::Dictionary(dict));
    }
    // Liens de parenté et de fratrie, racine comprise.
    for (i, item) in items.iter().enumerate() {
        let Some(parent) = ids[i] else {
            continue;
        };
        let children: Vec<ObjectId> = item
            .children
            .iter()
            .filter_map(|&c| if kept[c] { ids[c] } else { None })
            .collect();
        for (k, child) in children.iter().enumerate() {
            let Ok(dict) = dst.get_dictionary_mut(*child) else {
                continue;
            };
            dict.set("Parent", parent);
            if k > 0 {
                dict.set("Prev", children[k - 1]);
            }
            if let Some(next) = children.get(k + 1) {
                dict.set("Next", *next);
            }
        }
    }
    attach_to_root(dst, root_id, existing.is_some(), &top, &ids, visible[0]);
}

/// Accroche les signets de premier niveau `top` à la racine `root_id` : nouvelle racine, ou
/// à la suite des signets d'une source précédente (fusion).
fn attach_to_root(
    dst: &mut LoDocument,
    root_id: ObjectId,
    existing: bool,
    top: &[usize],
    ids: &[Option<ObjectId>],
    visible: i64,
) {
    let (Some(first), Some(last)) = (
        top.first().and_then(|&i| ids[i]),
        top.last().and_then(|&i| ids[i]),
    ) else {
        return;
    };
    let previous = existing
        .then(|| dst.get_dictionary(root_id).ok())
        .flatten()
        .map(|root| {
            (
                root.get(b"Last").and_then(Object::as_reference).ok(),
                root.get(b"Count").and_then(Object::as_i64).unwrap_or(0),
            )
        });
    let (previous_last, previous_count) = previous.unwrap_or((None, 0));
    if let Some(previous_last) = previous_last {
        if let Ok(dict) = dst.get_dictionary_mut(previous_last) {
            dict.set("Next", first);
        }
        if let Ok(dict) = dst.get_dictionary_mut(first) {
            dict.set("Prev", previous_last);
        }
    }
    let mut root = dst
        .get_dictionary(root_id)
        .ok()
        .filter(|_| existing)
        .cloned()
        .unwrap_or_default();
    root.set("Type", Object::Name(b"Outlines".to_vec()));
    if previous_last.is_none() {
        root.set("First", first);
    }
    root.set("Last", last);
    root.set("Count", previous_count.abs() + visible);
    dst.objects.insert(root_id, Object::Dictionary(root));
    if !existing && let Ok(catalog) = dst.catalog_mut() {
        catalog.set("Outlines", root_id);
    }
}
