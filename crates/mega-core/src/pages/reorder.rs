// SPDX-License-Identifier: GPL-3.0-or-later
//! Déplacement de pages par réordonnancement de l'arbre des pages (`/Kids`), audit E4.
//!
//! PDFium n'expose pas de déplacement : la copie d'une page en fait un **nouvel objet**, et tout
//! ce qui visait l'ancien (signets, destinations nommées, liens des autres pages, arbre de
//! structure, champ du formulaire) se retrouvait orphelin. Ici, on ne touche qu'à l'arbre des
//! pages (§7.7.3) : la page change de place dans les `/Kids` d'un nœud `/Pages`, mais reste le
//! même objet, donc chaque référence vers elle reste juste.
//!
//! \[Apprentissage\] **Fonction pure sur un `lopdf::Document`** : [`reorder_kids`] ne prend ni
//! fichier ni PDFium, seulement l'arbre d'objets en mémoire ; ses tests construisent un petit
//! document à la main (`dictionary!`) et vérifient le résultat sans rien rendre ni écrire.
//!
//! \[Apprentissage\] **Attributs hérités** (§7.7.3.4) : `/Resources`, `/MediaBox`, `/CropBox`
//! et `/Rotate` peuvent être posés sur un nœud `/Pages` et valent alors pour toutes les pages
//! en dessous. Une page qui change de parent perdrait ceux de l'ancien et prendrait ceux du
//! nouveau : on les recopie sur la page **avant** de la déplacer.

use std::collections::{BTreeSet, HashMap, HashSet};

use lopdf::{Dictionary, Document as LoDocument, Object, ObjectId};

use super::carry::MAX_DEPTH;
use crate::error::{CoreError, CoreResult};

/// Attributs de page héritables (§7.7.3.4). `/MediaBox` précède `/CropBox` : la valeur neutre
/// d'une `/CropBox` est la `/MediaBox` de la page, recopiée juste avant.
const INHERITED: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

/// L'arbre des pages tel que lu depuis le catalogue.
struct PageTree {
    /// Le nœud `/Pages` racine.
    root: ObjectId,
    /// Chaque page, dans l'ordre du document, avec le nœud qui la porte dans ses `/Kids`.
    pages: Vec<(ObjectId, ObjectId)>,
    /// Chaque nœud `/Pages` → son nœud parent (`None` pour la racine). Tiré des `/Kids`, pas
    /// des clés `/Parent`, qu'un fichier abîmé peut contredire.
    parents: HashMap<ObjectId, Option<ObjectId>>,
}

/// Ce que vaut un élément de `/Kids` (même règle que `lopdf::Document::get_pages`).
enum Kind {
    Node,
    Page,
    Ignored,
}

fn kind(doc: &LoDocument, id: ObjectId) -> Kind {
    match doc.get_dictionary(id).and_then(Dictionary::get_type) {
        Ok(b"Pages") => Kind::Node,
        Ok(b"Page") => Kind::Page,
        Ok(_) => Kind::Ignored,
        // Objet absent ou sans `/Type` : lopdf le compte comme une page (rendue blanche).
        Err(_) => Kind::Page,
    }
}

fn unsupported(why: &str) -> CoreError {
    CoreError::InvalidOperation(format!(
        "arbre des pages que cette version ne sait pas réordonner : {why}"
    ))
}

/// Les `/Kids` du nœud `node` (tableau direct ou référencé).
fn kids(doc: &LoDocument, node: ObjectId) -> Option<&Vec<Object>> {
    let kids = doc.get_dictionary(node).ok()?.get(b"Kids").ok()?;
    doc.dereference(kids).ok()?.1.as_array().ok()
}

/// Les `/Kids` du nœud `node`, modifiables.
fn kids_mut(doc: &mut LoDocument, node: ObjectId) -> CoreResult<&mut Vec<Object>> {
    let indirect = doc
        .get_dictionary(node)
        .ok()
        .and_then(|d| d.get(b"Kids").ok())
        .and_then(|k| k.as_reference().ok());
    let kids = match indirect {
        Some(id) => doc.get_object_mut(id),
        None => doc
            .get_dictionary_mut(node)
            .and_then(|d| d.get_mut(b"Kids")),
    };
    kids.and_then(Object::as_array_mut)
        .map_err(|_| unsupported("/Kids illisible"))
}

/// Lit l'arbre des pages. Pile explicite, profondeur bornée (`MAX_DEPTH`), chaque objet visité
/// une fois : un objet rencontré deux fois (cycle, page partagée) rend le déplacement ambigu et
/// est refusé.
fn read_tree(doc: &LoDocument) -> CoreResult<PageTree> {
    let root = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"Pages").ok())
        .and_then(|p| p.as_reference().ok())
        .ok_or_else(|| unsupported("catalogue sans /Pages"))?;
    let mut tree = PageTree {
        root,
        pages: Vec::new(),
        parents: HashMap::new(),
    };
    let mut seen = HashSet::new();
    // (objet, nœud qui le porte, profondeur) ; les enfants sont empilés à l'envers pour être
    // dépilés dans l'ordre du document.
    let mut stack: Vec<(ObjectId, Option<ObjectId>, usize)> = vec![(root, None, 0)];
    while let Some((id, parent, depth)) = stack.pop() {
        if !seen.insert(id) {
            return Err(unsupported("objet cité deux fois"));
        }
        match (kind(doc, id), parent) {
            (Kind::Node, _) => {
                if depth >= MAX_DEPTH {
                    return Err(unsupported("arbre trop profond"));
                }
                tree.parents.insert(id, parent);
                let children = kids(doc, id).ok_or_else(|| unsupported("/Kids illisible"))?;
                for kid in children.iter().rev() {
                    if let Ok(kid) = kid.as_reference() {
                        stack.push((kid, Some(id), depth + 1));
                    }
                }
            }
            (Kind::Page, Some(parent)) => tree.pages.push((id, parent)),
            (Kind::Page, None) => return Err(unsupported("racine qui n'est pas un /Pages")),
            (Kind::Ignored, _) => {}
        }
    }
    Ok(tree)
}

/// Valeur de `key` posée sur `node` ou sur l'un de ses ancêtres (le plus proche l'emporte).
fn inherited<'a>(
    doc: &'a LoDocument,
    tree: &PageTree,
    node: ObjectId,
    key: &[u8],
) -> Option<&'a Object> {
    let mut current = Some(node);
    // La chaîne des parents vient de `read_tree` : sans cycle et de longueur ≤ `MAX_DEPTH`.
    while let Some(id) = current {
        if let Ok(value) = doc.get_dictionary(id).and_then(|d| d.get(key)) {
            return Some(value);
        }
        current = tree.parents.get(&id).copied().flatten();
    }
    None
}

/// Fige sur `page` les attributs hérités de `old_parent`, et neutralise ceux que `new_parent`
/// lui ferait hériter à tort (`/Rotate 0`, `/Resources` vide, `/CropBox` = `/MediaBox`).
fn pin_inherited(
    doc: &mut LoDocument,
    tree: &PageTree,
    page: ObjectId,
    old_parent: ObjectId,
    new_parent: ObjectId,
) {
    for key in INHERITED {
        let Ok(own) = doc.get_dictionary(page) else {
            return;
        };
        if own.has(key) {
            continue;
        }
        let value = match inherited(doc, tree, old_parent, key) {
            Some(value) => Some(value.clone()),
            None if inherited(doc, tree, new_parent, key).is_some() => match key {
                b"Rotate" => Some(Object::Integer(0)),
                b"Resources" => Some(Object::Dictionary(Dictionary::new())),
                b"CropBox" => own.get(b"MediaBox").ok().cloned(),
                // `/MediaBox` est obligatoire : sans valeur d'origine, rien à recopier.
                _ => None,
            },
            None => None,
        };
        if let (Some(value), Ok(own)) = (value, doc.get_dictionary_mut(page)) {
            own.set(key.to_vec(), value);
        }
    }
}

/// Décroche de leur parent les nœuds (hors racine) que le déplacement a vidés, en remontant.
fn unhook_empty_nodes(doc: &mut LoDocument, tree: &PageTree, emptied: &BTreeSet<ObjectId>) {
    for &start in emptied {
        let mut node = start;
        while node != tree.root && kids(doc, node).is_some_and(Vec::is_empty) {
            let Some(Some(parent)) = tree.parents.get(&node).copied() else {
                break;
            };
            if let Ok(siblings) = kids_mut(doc, parent) {
                siblings.retain(|k| k.as_reference().ok() != Some(node));
            }
            node = parent;
        }
    }
}

/// Recalcule le `/Count` (nombre de pages en dessous) de chaque nœud de l'arbre.
fn recount(doc: &mut LoDocument) -> CoreResult<()> {
    let tree = read_tree(doc)?;
    let mut counts: HashMap<ObjectId, i64> = tree.parents.keys().map(|&n| (n, 0)).collect();
    for &(_, parent) in &tree.pages {
        let mut current = Some(parent);
        while let Some(node) = current {
            if let Some(count) = counts.get_mut(&node) {
                *count += 1;
            }
            current = tree.parents.get(&node).copied().flatten();
        }
    }
    for (node, count) in counts {
        if let Ok(dict) = doc.get_dictionary_mut(node) {
            dict.set("Count", Object::Integer(count));
        }
    }
    Ok(())
}

/// Déplace les pages `pages` (index 0-based, pris dans l'ordre du document) pour qu'elles
/// commencent à l'index `to`, **compté une fois ces pages retirées**
/// (`0..=page_count - pages.len()`), en réordonnant les `/Kids` de l'arbre des pages.
///
/// Pourquoi : les pages restent les mêmes objets, si bien que signets, destinations, liens,
/// balisage et formulaire qui les visent restent valables (audit E4) ; aucune référence n'est
/// à réécrire. Les pages vont se ranger à côté de leur nouvelle voisine (après la page
/// d'index `to - 1`, ou avant la première si `to == 0`) ; une page qui change de nœud parent
/// emporte ses attributs hérités, un nœud vidé est décroché et chaque `/Count` recalculé.
///
/// Les arguments et l'arbre sont vérifiés avant toute modification ; une erreur plus tardive
/// (structure que la lecture tolérait) peut laisser `doc` modifié en partie : l'appelant
/// travaille sur une copie qu'il abandonne alors.
pub(crate) fn reorder_kids(doc: &mut LoDocument, pages: &[u16], to: u16) -> CoreResult<()> {
    let tree = read_tree(doc)?;
    // Notre lecture et celle de lopdf (comparée à PDFium par l'appelant) doivent voir les
    // mêmes pages, sinon les index ne désigneraient pas les mêmes.
    if tree.pages.len() != doc.get_pages().len() {
        return Err(unsupported("pages comptées différemment"));
    }
    let count = tree.pages.len();
    let moved: BTreeSet<usize> = pages.iter().map(|&p| usize::from(p)).collect();
    if moved.is_empty() {
        return Err(CoreError::InvalidOperation(
            "aucune page sélectionnée".to_string(),
        ));
    }
    if let Some(&index) = moved.iter().find(|&&p| p >= count) {
        return Err(CoreError::PageOutOfRange {
            index: u16::try_from(index).unwrap_or(u16::MAX),
            count: u16::try_from(count).unwrap_or(u16::MAX),
        });
    }
    // [Apprentissage] `partition` coupe un itérateur en deux collections selon un prédicat.
    let (moving, remaining): (Vec<_>, Vec<_>) = tree
        .pages
        .iter()
        .enumerate()
        .partition(|(i, _)| moved.contains(i));
    let moving: Vec<(ObjectId, ObjectId)> = moving.into_iter().map(|(_, &p)| p).collect();
    let remaining: Vec<(ObjectId, ObjectId)> = remaining.into_iter().map(|(_, &p)| p).collect();
    let to = usize::from(to);
    if to > remaining.len() {
        return Err(CoreError::PageOutOfRange {
            index: u16::try_from(to).unwrap_or(u16::MAX),
            count: u16::try_from(remaining.len()).unwrap_or(u16::MAX),
        });
    }
    // La voisine : la page qui précédera le groupe, ou celle qui le suivra s'il passe en tête.
    let (anchor, after) = match to.checked_sub(1) {
        Some(before) => (remaining.get(before), true),
        None => (remaining.first(), false),
    };
    // Toutes les pages déplacées : l'ordre ne change pas.
    let Some(&(anchor, new_parent)) = anchor else {
        return Ok(());
    };
    // Ordre inchangé (la sélection est déjà à sa place) : rien à écrire.
    let mut order: Vec<ObjectId> = remaining.iter().map(|(p, _)| *p).collect();
    order.splice(to..to, moving.iter().map(|(p, _)| *p));
    if order.iter().eq(tree.pages.iter().map(|(p, _)| p)) {
        return Ok(());
    }

    // 1. Attributs hérités figés sur les pages qui changent de parent, avant de les détacher.
    for &(page, old_parent) in &moving {
        if old_parent != new_parent {
            pin_inherited(doc, &tree, page, old_parent, new_parent);
        }
    }
    // 2. Détacher les pages de leur parent.
    let mut emptied = BTreeSet::new();
    for &(page, old_parent) in &moving {
        kids_mut(doc, old_parent)?.retain(|k| k.as_reference().ok() != Some(page));
        emptied.insert(old_parent);
    }
    // 3. Les insérer, d'un bloc, à côté de la voisine.
    let siblings = kids_mut(doc, new_parent)?;
    let at = siblings
        .iter()
        .position(|k| k.as_reference().ok() == Some(anchor))
        .ok_or_else(|| unsupported("page introuvable dans les /Kids de son parent"))?;
    let at = if after { at + 1 } else { at };
    siblings.splice(at..at, moving.iter().map(|(p, _)| Object::Reference(*p)));
    for &(page, _) in &moving {
        if let Ok(dict) = doc.get_dictionary_mut(page) {
            dict.set("Parent", Object::Reference(new_parent));
        }
    }
    // 4. Nœuds vidés décrochés (l'élagage de l'enregistrement les retire), puis recomptage.
    unhook_empty_nodes(doc, &tree, &emptied);
    recount(doc)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use lopdf::{Document as LoDocument, Object, ObjectId, dictionary};

    use super::*;
    use crate::error::CoreError;

    /// Arbre à deux niveaux : racine (2) → A (3 : `/Rotate 90`, `/MediaBox`, `/Resources`)
    /// et B (4) ; A porte les pages 5 et 6, B les pages 7 et 8.
    fn two_level() -> LoDocument {
        let mut doc = LoDocument::with_version("1.7");
        let page = |parent: u32| {
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => Object::Reference((parent, 0)),
            })
        };
        doc.objects.insert(
            (1, 0),
            Object::Dictionary(
                dictionary! { "Type" => "Catalog", "Pages" => Object::Reference((2, 0)) },
            ),
        );
        doc.objects.insert(
            (2, 0),
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference((3, 0)), Object::Reference((4, 0))],
                "Count" => 4,
            }),
        );
        doc.objects.insert(
            (3, 0),
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Parent" => Object::Reference((2, 0)),
                "Kids" => vec![Object::Reference((5, 0)), Object::Reference((6, 0))],
                "Count" => 2,
                "Rotate" => 90,
                "MediaBox" => vec![0.into(), 0.into(), 200.into(), 300.into()],
                "Resources" => dictionary! { "Font" => dictionary! {} },
            }),
        );
        doc.objects.insert(
            (4, 0),
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Parent" => Object::Reference((2, 0)),
                "Kids" => vec![Object::Reference((7, 0)), Object::Reference((8, 0))],
                "Count" => 2,
            }),
        );
        for (id, parent) in [(5, 3), (6, 3), (7, 4), (8, 4)] {
            doc.objects.insert((id, 0), page(parent));
        }
        doc.trailer.set("Root", Object::Reference((1, 0)));
        doc.max_id = 8;
        doc
    }

    fn order(doc: &LoDocument) -> Vec<u32> {
        doc.get_pages().values().map(|id| id.0).collect()
    }

    fn count(doc: &LoDocument, node: u32) -> i64 {
        doc.get_dictionary((node, 0))
            .unwrap()
            .get(b"Count")
            .unwrap()
            .as_i64()
            .unwrap()
    }

    fn parent(doc: &LoDocument, page: u32) -> ObjectId {
        doc.get_dictionary((page, 0))
            .unwrap()
            .get(b"Parent")
            .unwrap()
            .as_reference()
            .unwrap()
    }

    #[test]
    fn reorder_kids_moves_a_page_and_keeps_counts() {
        let mut doc = two_level();
        // Page 1 (5) à l'index 2, compté une fois retirée : après la page 3 (7).
        reorder_kids(&mut doc, &[0], 2).unwrap();
        assert_eq!(order(&doc), [6, 7, 5, 8]);
        assert_eq!(count(&doc, 2), 4, "racine");
        assert_eq!(count(&doc, 3), 1, "A a perdu une page");
        assert_eq!(count(&doc, 4), 3, "B en a gagné une");
        assert_eq!(parent(&doc, 5), (4, 0));
    }

    #[test]
    fn reorder_kids_copies_inherited_attributes_before_reparenting() {
        let mut doc = two_level();
        reorder_kids(&mut doc, &[0], 2).unwrap();
        let moved = doc.get_dictionary((5, 0)).unwrap();
        assert_eq!(moved.get(b"Rotate").unwrap().as_i64().unwrap(), 90);
        let media: Vec<i64> = moved
            .get(b"MediaBox")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_i64().unwrap())
            .collect();
        assert_eq!(media, [0, 0, 200, 300]);
        assert!(moved.has(b"Resources"));
        // Les pages restées sous leur parent n'ont rien reçu.
        assert!(!doc.get_dictionary((6, 0)).unwrap().has(b"Rotate"));
        assert!(!doc.get_dictionary((7, 0)).unwrap().has(b"Rotate"));
    }

    #[test]
    fn a_page_moved_under_a_rotated_parent_keeps_its_own_orientation() {
        let mut doc = two_level();
        // Page 3 (7, sans rotation) vers A, qui porte `/Rotate 90` : la page doit rester droite.
        reorder_kids(&mut doc, &[2], 0).unwrap();
        assert_eq!(order(&doc), [7, 5, 6, 8]);
        let moved = doc.get_dictionary((7, 0)).unwrap();
        assert_eq!(moved.get(b"Rotate").unwrap().as_i64().unwrap(), 0);
        assert!(moved.get(b"Resources").unwrap().as_dict().is_ok());
        assert_eq!(parent(&doc, 7), (3, 0));
    }

    #[test]
    fn an_emptied_intermediate_node_is_unhooked() {
        let mut doc = two_level();
        reorder_kids(&mut doc, &[0, 1], 2).unwrap();
        assert_eq!(order(&doc), [7, 8, 5, 6]);
        let root_kids = doc
            .get_dictionary((2, 0))
            .unwrap()
            .get(b"Kids")
            .unwrap()
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(root_kids, [Object::Reference((4, 0))]);
        assert_eq!(count(&doc, 2), 4);
        assert_eq!(count(&doc, 4), 4);
    }

    #[test]
    fn moving_every_page_or_to_the_same_place_changes_nothing() {
        for (pages, to) in [(vec![0, 1, 2, 3], 0), (vec![1], 1), (vec![0, 1], 0)] {
            let mut doc = two_level();
            let before = doc.objects.clone();
            reorder_kids(&mut doc, &pages, to).unwrap();
            assert_eq!(doc.objects, before, "{pages:?} → {to}");
        }
    }

    #[test]
    fn out_of_range_arguments_are_refused() {
        let mut doc = two_level();
        assert!(matches!(
            reorder_kids(&mut doc, &[4], 0),
            Err(CoreError::PageOutOfRange { index: 4, count: 4 })
        ));
        assert!(matches!(
            reorder_kids(&mut doc, &[0], 4),
            Err(CoreError::PageOutOfRange { index: 4, count: 3 })
        ));
        assert!(matches!(
            reorder_kids(&mut doc, &[], 0),
            Err(CoreError::InvalidOperation(_))
        ));
    }

    #[test]
    fn a_cyclic_page_tree_is_refused_without_looping() {
        let mut doc = two_level();
        // B se cite lui-même dans ses `/Kids`.
        doc.get_dictionary_mut((4, 0)).unwrap().set(
            "Kids",
            vec![Object::Reference((7, 0)), Object::Reference((4, 0))],
        );
        assert!(matches!(
            reorder_kids(&mut doc, &[0], 1),
            Err(CoreError::InvalidOperation(_))
        ));
    }
}
