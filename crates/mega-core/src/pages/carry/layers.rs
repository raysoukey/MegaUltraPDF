// SPDX-License-Identifier: GPL-3.0-or-later
//! Calques (contenu optionnel, `/OCProperties`, §8.11.4) des pages copiées (audit E3).
//!
//! PDFium copie les groupes de contenu optionnel (OCG) que citent les pages (`/Properties`
//! des ressources, `/OC` des XObjects et des annotations), mais pas `/OCProperties`, qui dit
//! lesquels existent et lesquels sont **masqués** à l'ouverture : sans lui, un calque masqué
//! devient visible. On le reconstruit pour les seuls OCG copiés.

use std::collections::HashMap;

use lopdf::{Dictionary, Document as LoDocument, Object, ObjectId};

use super::{MAX_DEPTH, catalog_dict};

/// Références de `array` (déréférencé), dans l'ordre.
fn references(doc: &LoDocument, array: Option<&Object>) -> Vec<ObjectId> {
    array
        .and_then(|a| doc.dereference(a).ok())
        .and_then(|(_, a)| a.as_array().ok())
        .map(|a| a.iter().filter_map(|r| r.as_reference().ok()).collect())
        .unwrap_or_default()
}

/// `/Order` (arborescence d'affichage des calques) réduit aux OCG copiés, réécrits vers leur
/// copie ; les libellés (chaînes) et sous-listes restent. Pile explicite, profondeur bornée.
fn filtered_order(
    doc: &LoDocument,
    order: &Object,
    copies: &HashMap<ObjectId, ObjectId>,
) -> Vec<Object> {
    let Some(mut order) = doc
        .dereference(order)
        .ok()
        .and_then(|(_, o)| o.as_array().ok())
        .cloned()
    else {
        return Vec::new();
    };
    // [Apprentissage] Pile de `&mut` : chaque emprunt dépilé est consommé, et les sous-listes
    // empilées sont des parties disjointes de la liste qu'il désignait (voir
    // `carry::visit_references`).
    let mut stack: Vec<(&mut Vec<Object>, usize)> = vec![(&mut order, 0)];
    while let Some((items, depth)) = stack.pop() {
        items.retain_mut(|item| match item {
            Object::Reference(id) => match copies.get(id) {
                Some(&copy) => {
                    *id = copy;
                    true
                }
                None => false,
            },
            Object::Array(_) => depth < MAX_DEPTH,
            Object::String(..) => true,
            _ => false,
        });
        for item in items.iter_mut() {
            if let Object::Array(sub) = item {
                stack.push((sub, depth + 1));
            }
        }
    }
    order
}

/// Ajoute à `config[key]` (tableau) les éléments `added`.
fn append(config: &mut Dictionary, key: &str, added: Vec<Object>) {
    if added.is_empty() {
        return;
    }
    let mut items = config
        .get(key.as_bytes())
        .and_then(Object::as_array)
        .cloned()
        .unwrap_or_default();
    items.extend(added);
    config.set(key, items);
}

/// Reconstruit `/OCProperties` pour les OCG de la source que PDFium a copiés (`cache`) :
/// `/OCGs`, et la configuration par défaut `/D` (`/ON`, `/OFF` explicites selon l'état
/// d'ouverture dans la source, `/Order`, `/RBGroups`, `/Locked` réduits aux copies). Lors
/// d'une fusion, ces entrées complètent celles d'une source précédente. Rien n'est écrit si
/// aucune page copiée n'utilise de calque.
pub(crate) fn carry_oc_properties(
    src: &LoDocument,
    dst: &mut LoDocument,
    cache: &HashMap<ObjectId, ObjectId>,
) {
    let Some(properties) = catalog_dict(src, b"OCProperties") else {
        return;
    };
    let copies: HashMap<ObjectId, ObjectId> = references(src, properties.get(b"OCGs").ok())
        .into_iter()
        .filter_map(|ocg| cache.get(&ocg).map(|&copy| (ocg, copy)))
        .collect();
    if copies.is_empty() {
        return;
    }
    let source = properties
        .get(b"D")
        .ok()
        .and_then(|d| src.dereference(d).ok())
        .and_then(|(_, d)| d.as_dict().ok())
        .cloned()
        .unwrap_or_default();
    // État à l'ouverture dans la source : `/BaseState` (ON par défaut), corrigé par `/ON` et
    // `/OFF`. Il est écrit explicitement : la configuration de `dst` peut avoir une autre base.
    let base_off = source.get(b"BaseState").and_then(Object::as_name).ok() == Some(b"OFF");
    let on = references(src, source.get(b"ON").ok());
    let off = references(src, source.get(b"OFF").ok());
    let mut ocgs = Vec::new();
    let (mut shown, mut hidden) = (Vec::new(), Vec::new());
    for ocg in references(src, properties.get(b"OCGs").ok()) {
        let Some(&copy) = copies.get(&ocg) else {
            continue;
        };
        if ocgs.contains(&Object::Reference(copy)) {
            continue;
        }
        ocgs.push(Object::Reference(copy));
        if off.contains(&ocg) || (base_off && !on.contains(&ocg)) {
            hidden.push(Object::Reference(copy));
        } else {
            shown.push(Object::Reference(copy));
        }
    }
    let order = source
        .get(b"Order")
        .map(|o| filtered_order(src, o, &copies))
        .unwrap_or_default();
    let groups: Vec<Object> = source
        .get(b"RBGroups")
        .ok()
        .and_then(|g| src.dereference(g).ok())
        .and_then(|(_, g)| g.as_array().ok())
        .map(|groups| {
            groups
                .iter()
                .map(|g| references(src, Some(g)))
                .map(|g| {
                    g.iter()
                        .filter_map(|ocg| copies.get(ocg).copied().map(Object::Reference))
                        .collect::<Vec<Object>>()
                })
                .filter(|g| !g.is_empty())
                .map(Object::Array)
                .collect()
        })
        .unwrap_or_default();
    let locked: Vec<Object> = references(src, source.get(b"Locked").ok())
        .iter()
        .filter_map(|ocg| copies.get(ocg).copied().map(Object::Reference))
        .collect();

    let mut target = catalog_dict(dst, b"OCProperties")
        .cloned()
        .unwrap_or_default();
    let mut config = target
        .get(b"D")
        .ok()
        .and_then(|d| dst.dereference(d).ok())
        .and_then(|(_, d)| d.as_dict().ok())
        .cloned()
        .unwrap_or_default();
    if !config.has(b"Name")
        && let Ok(name @ Object::String(..)) = source.get(b"Name")
    {
        config.set("Name", name.clone());
    }
    append(&mut target, "OCGs", ocgs);
    append(&mut config, "ON", shown);
    append(&mut config, "OFF", hidden);
    append(&mut config, "Order", order);
    append(&mut config, "RBGroups", groups);
    append(&mut config, "Locked", locked);
    target.set("D", config);
    if let Ok(catalog) = dst.catalog_mut() {
        catalog.set("OCProperties", target);
    }
}
