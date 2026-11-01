// SPDX-License-Identifier: GPL-3.0-or-later
//! Formulaire (`/AcroForm`) des pages copiées (audit E3).
//!
//! PDFium copie les widgets (ce sont des annotations de page) mais pas le formulaire qui les
//! déclare, ni les dictionnaires de champ parents (`/Parent` n'est pas renuméroté). On
//! reconstruit ici l'arbre des champs qui ont au moins un widget copié : les champs parents
//! sont clonés, les widgets sont ceux que PDFium a déjà copiés, et chaque lien
//! parent ↔ enfant est réécrit.

use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Document as LoDocument, Object, ObjectId};

use super::{MAX_DEPTH, PageMap, catalog_dict, clone_object};

/// Un nœud de l'arbre des champs de la source.
struct Node {
    /// Son dictionnaire dans la source.
    id: ObjectId,
    /// Son parent (index dans la liste des nœuds), `None` au premier niveau.
    parent: Option<usize>,
    /// Ses enfants, dans l'ordre de `/Kids`.
    kids: Vec<usize>,
    /// Pour un widget, sa copie par PDFium (absente si sa page n'a pas été copiée).
    widget_copy: Option<ObjectId>,
}

/// Les champs de `/AcroForm /Fields`, à plat : chaque enfant après son parent (pile
/// explicite, chaque nœud visité une fois, profondeur bornée).
fn read_fields(
    src: &LoDocument,
    form: &Dictionary,
    cache: &HashMap<ObjectId, ObjectId>,
) -> Vec<Node> {
    let mut nodes: Vec<Node> = Vec::new();
    let Ok(roots) = form.get(b"Fields").and_then(Object::as_array) else {
        return nodes;
    };
    let mut seen = HashSet::new();
    let mut stack: Vec<(ObjectId, Option<usize>, usize)> = roots
        .iter()
        .rev()
        .filter_map(|r| r.as_reference().ok())
        .map(|id| (id, None, 0))
        .collect();
    while let Some((id, parent, depth)) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Ok(dict) = src.get_dictionary(id) else {
            continue;
        };
        let is_widget = dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Widget");
        let index = nodes.len();
        nodes.push(Node {
            id,
            parent,
            kids: Vec::new(),
            widget_copy: if is_widget {
                cache.get(&id).copied()
            } else {
                None
            },
        });
        if let Some(parent) = parent {
            nodes[parent].kids.push(index);
        }
        if depth < MAX_DEPTH
            && let Ok(kids) = dict.get(b"Kids").and_then(Object::as_array)
        {
            stack.extend(
                kids.iter()
                    .rev()
                    .filter_map(|k| k.as_reference().ok())
                    .map(|k| (k, Some(index), depth + 1)),
            );
        }
    }
    nodes
}

/// Texte du `/T` d'un champ.
fn field_name(dict: &Dictionary) -> Option<String> {
    let name = dict.get(b"T").and_then(Object::as_str).ok()?;
    Some(crate::properties::decode_text_string(name))
}

/// Reporte le formulaire : `/DA`, `/DR`, `/NeedAppearances` et `/Fields` réduit aux champs
/// dont au moins un widget (le champ lui-même ou un de ses `/Kids`) est sur une page copiée.
/// Les champs parents sont clonés, chaque widget copié reçoit le `/Parent` cloné. Lors d'une
/// fusion, un champ de premier niveau dont le nom existe déjà dans `dst` reçoit `suffix`
/// (`nom` → `nom (2)`) : deux champs de même nom qualifié n'en feraient qu'un (§12.7.3.2).
pub(crate) fn carry_acroform(
    src: &LoDocument,
    dst: &mut LoDocument,
    map: &PageMap,
    cache: &mut HashMap<ObjectId, ObjectId>,
    suffix: Option<&str>,
) {
    let Some(form) = catalog_dict(src, b"AcroForm") else {
        return;
    };
    let nodes = read_fields(src, form, cache);
    // Un champ survit s'il est un widget copié ou si un de ses enfants survit ; les enfants
    // suivent leur parent dans la liste, on remonte donc depuis la fin.
    let mut alive = vec![false; nodes.len()];
    for i in (0..nodes.len()).rev() {
        alive[i] = nodes[i].widget_copy.is_some() || nodes[i].kids.iter().any(|&k| alive[k]);
    }
    if !alive.iter().any(|&a| a) {
        return;
    }
    // Numéro dans `dst` : la copie de PDFium pour un widget, un nouveau numéro pour un champ.
    let targets: Vec<Option<ObjectId>> = nodes
        .iter()
        .zip(&alive)
        .map(|(node, &alive)| {
            if !alive {
                return None;
            }
            let target = node.widget_copy.unwrap_or_else(|| dst.new_object_id());
            cache.insert(node.id, target);
            Some(target)
        })
        .collect();
    for (i, node) in nodes.iter().enumerate() {
        let Some(target) = targets[i] else {
            continue;
        };
        let parent = node.parent.and_then(|p| targets[p]);
        let kids: Vec<Object> = node
            .kids
            .iter()
            .filter_map(|&k| targets[k].map(Object::Reference))
            .collect();
        if node.widget_copy.is_none() {
            let Ok(original) = src.get_dictionary(node.id) else {
                continue;
            };
            let mut field = Dictionary::new();
            for (key, value) in original.iter() {
                // `/AA` : pas d'import de script (formatage, frappe, calcul).
                if !matches!(key.as_slice(), b"Kids" | b"Parent" | b"AA") {
                    let value = clone_object(src, dst, value, map, cache);
                    field.set(key.clone(), value);
                }
            }
            field.set("Kids", kids);
            dst.objects.insert(target, Object::Dictionary(field));
        }
        if let Ok(dict) = dst.get_dictionary_mut(target) {
            // Le widget copié par PDFium garde ses scripts : on ne les importe pas.
            dict.remove(b"AA");
            match parent {
                Some(parent) => dict.set("Parent", parent),
                None => {
                    dict.remove(b"Parent");
                }
            }
        }
    }
    let roots: Vec<ObjectId> = nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.parent.is_none())
        .filter_map(|(i, _)| targets[i])
        .collect();
    write_form(src, dst, form, map, cache, &roots, suffix);
}

/// Écrit `/AcroForm` dans `dst` : les champs `roots` à la suite de ceux d'une source
/// précédente (renommés avec `suffix` en cas de collision), et les entrées par défaut de la
/// source quand `dst` ne les a pas encore.
fn write_form(
    src: &LoDocument,
    dst: &mut LoDocument,
    form: &Dictionary,
    map: &PageMap,
    cache: &mut HashMap<ObjectId, ObjectId>,
    roots: &[ObjectId],
    suffix: Option<&str>,
) {
    let mut target = catalog_dict(dst, b"AcroForm").cloned().unwrap_or_default();
    let mut fields: Vec<Object> = target
        .get(b"Fields")
        .and_then(Object::as_array)
        .cloned()
        .unwrap_or_default();
    let mut taken: Vec<String> = fields
        .iter()
        .filter_map(|f| f.as_reference().ok())
        .filter_map(|id| dst.get_dictionary(id).ok())
        .filter_map(field_name)
        .collect();
    for &root in roots {
        let name = dst.get_dictionary(root).ok().and_then(field_name);
        if let (Some(name), Some(suffix)) = (name, suffix)
            && taken.contains(&name)
        {
            let mut renamed = format!("{name}{suffix}");
            while taken.contains(&renamed) {
                renamed.push_str(suffix);
            }
            if let Ok(dict) = dst.get_dictionary_mut(root) {
                dict.set("T", crate::forms::write::pdf_text(&renamed));
            }
            taken.push(renamed);
        } else if let Some(name) = dst.get_dictionary(root).ok().and_then(field_name) {
            taken.push(name);
        }
        fields.push(Object::Reference(root));
    }
    target.set("Fields", fields);
    for key in [&b"DA"[..], b"NeedAppearances"] {
        if !target.has(key)
            && let Ok(value) = form.get(key)
        {
            let value = clone_object(src, dst, value, map, cache);
            target.set(key.to_vec(), value);
        }
    }
    carry_default_resources(src, dst, form, &mut target, map, cache);
    if let Ok(catalog) = dst.catalog_mut() {
        catalog.set("AcroForm", target);
    }
}

/// `/DR` (ressources par défaut du formulaire) : copié si `dst` n'en a pas, sinon complété
/// des catégories et des polices de la source qui y manquent (une fusion garde ainsi la police
/// que citent les `/DA` de chaque source).
fn carry_default_resources(
    src: &LoDocument,
    dst: &mut LoDocument,
    form: &Dictionary,
    target: &mut Dictionary,
    map: &PageMap,
    cache: &mut HashMap<ObjectId, ObjectId>,
) {
    let Some(source) = form.get(b"DR").ok().and_then(|r| deref_dict(src, r)) else {
        return;
    };
    let mut merged = target
        .get(b"DR")
        .ok()
        .and_then(|r| deref_dict(dst, r))
        .cloned()
        .unwrap_or_default();
    for (key, value) in source.iter() {
        if key.as_slice() == b"Font"
            && let Some(mut fonts) = merged
                .get(b"Font")
                .ok()
                .and_then(|f| deref_dict(dst, f))
                .cloned()
        {
            for (name, font) in deref_dict(src, value)
                .map(Dictionary::iter)
                .into_iter()
                .flatten()
            {
                if !fonts.has(name) {
                    let font = clone_object(src, dst, font, map, cache);
                    fonts.set(name.clone(), font);
                }
            }
            merged.set("Font", fonts);
        } else if !merged.has(key) {
            let value = clone_object(src, dst, value, map, cache);
            merged.set(key.clone(), value);
        }
    }
    target.set("DR", merged);
}

/// Dictionnaire désigné par `object` (direct ou référencé) dans `doc`.
fn deref_dict<'a>(doc: &'a LoDocument, object: &'a Object) -> Option<&'a Dictionary> {
    doc.dereference(object).ok()?.1.as_dict().ok()
}
