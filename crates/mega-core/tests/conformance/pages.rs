// SPDX-License-Identifier: GPL-3.0-or-later
//! Fusion, scission, extraction et déplacements dans l'arbre des pages.

use mega_core::Document;
use mega_core::edit::EditCommand;

use crate::{assert_conforms, fixture, qpdf};

/// Fusion, scission et extraction assemblent un document neuf, puis y reportent le catalogue
/// des sources (Lot 14, audit E3) : formulaire, signets, pièces jointes, étiquettes, calques.
#[test]
fn merge_split_and_extract_produce_conforming_files() {
    let Some(qpdf) = qpdf() else { return };
    use mega_core::pages::{merge, split};
    use mega_core::testing;
    let dir = tempfile::tempdir().unwrap();
    let open = |bytes: Vec<u8>| Document::from_bytes(bytes, None).unwrap();
    let rich = open(testing::rich_catalog_pdf("1.7", None));
    let form = open(testing::form_pdf());
    let parent_kid = open(testing::form_parent_kid_pdf());
    let simple = Document::open(fixture("simple.pdf"), None).unwrap();
    let mut outputs: Vec<(String, Vec<u8>)> = vec![
        (
            "fusion riche + riche".into(),
            merge(&[&rich, &rich]).unwrap(),
        ),
        (
            "fusion formulaire + formulaire".into(),
            merge(&[&form, &form]).unwrap(),
        ),
        (
            "fusion simple + parent/enfant".into(),
            merge(&[&simple, &parent_kid]).unwrap(),
        ),
        ("extraction p2".into(), rich.extract_pages(&[1]).unwrap()),
        (
            "extraction p3, p1".into(),
            rich.extract_pages(&[2, 0]).unwrap(),
        ),
    ];
    for (i, part) in split(&rich, &[vec![0], vec![1, 2]])
        .unwrap()
        .into_iter()
        .enumerate()
    {
        outputs.push((format!("scission, partie {}", i + 1), part));
    }
    for (what, bytes) in outputs {
        let path = dir.path().join("sortie.pdf");
        std::fs::write(&path, bytes).unwrap();
        assert_conforms(&qpdf, &path, None, &what);
    }
}

/// Déplacer une page d'un catalogue riche réordonne l'arbre des pages (`/Kids`) ; supprimer
/// une page neutralise les destinations qui la visaient (Lot 14, audit E4).
#[test]
fn moving_and_deleting_rich_pages_produce_conforming_files() {
    let Some(qpdf) = qpdf() else { return };
    use serde_json::json;
    let dir = tempfile::tempdir().unwrap();
    for (what, json) in [
        (
            "déplacer p1 en fin",
            json!({"kind": "move_pages", "pages": [0], "to": 2}),
        ),
        (
            "supprimer p2",
            json!({"kind": "delete_pages", "pages": [1]}),
        ),
    ] {
        let mut doc =
            Document::from_bytes(mega_core::testing::rich_catalog_pdf("1.7", None), None).unwrap();
        let command: EditCommand = serde_json::from_value(json).unwrap();
        doc.apply(&command).unwrap();
        let out = dir.path().join("sortie.pdf");
        doc.save_as(&out).unwrap();
        assert_conforms(&qpdf, &out, None, what);
    }
}

/// Déplacer des pages **d'un nœud `/Pages` à l'autre** (`two_level_pages_pdf`) : chaque sortie
/// passe `qpdf --check`, et chaque page garde la taille et l'orientation qu'elle avait, alors
/// qu'elle quitte les attributs hérités de son ancien nœud (audit E4).
#[test]
fn moving_pages_across_page_tree_nodes_produces_conforming_files() {
    let Some(qpdf) = qpdf() else { return };
    let dir = tempfile::tempdir().unwrap();
    let source = mega_core::testing::two_level_pages_pdf();
    let original = Document::from_bytes(source.clone(), None).unwrap();
    let before: Vec<_> = (0..4)
        .map(|i| {
            (
                original.page_rotation(i).unwrap(),
                original.page_size(i).unwrap(),
            )
        })
        .collect();
    let cases: [(&str, &[u16], u16); 4] = [
        ("page 1 vers la fin (nœud A → B)", &[0], 3),
        ("page 4 en tête (nœud B → A)", &[3], 0),
        ("sélection [1, 3] croisant les nœuds", &[0, 2], 2),
        ("nœud A vidé", &[0, 1], 2),
    ];
    for (what, pages, to) in cases {
        let mut doc = Document::from_bytes(source.clone(), None).unwrap();
        let json = serde_json::json!({"kind": "move_pages", "pages": pages, "to": to});
        let command: EditCommand = serde_json::from_value(json).unwrap();
        doc.apply(&command).unwrap();
        let out = dir.path().join("sortie.pdf");
        doc.save_as(&out).unwrap();
        assert_conforms(&qpdf, &out, None, what);
        // Ordre attendu : les pages restantes, le groupe déplacé inséré à `to`.
        let mut order: Vec<u16> = (0..4).filter(|i| !pages.contains(i)).collect();
        let at = usize::from(to);
        order.splice(at..at, pages.iter().copied());
        let reopened = Document::open(&out, None).unwrap();
        assert_eq!(reopened.page_count(), 4, "{what}");
        for (position, &origin) in order.iter().enumerate() {
            let position = position as u16;
            let text = reopened.page_text(position).unwrap();
            assert!(
                text.contains(&format!("Page {}", origin + 1)),
                "{what} : page {position} = « {text} »"
            );
            let after = (
                reopened.page_rotation(position).unwrap(),
                reopened.page_size(position).unwrap(),
            );
            assert_eq!(
                after,
                before[usize::from(origin)],
                "{what} : rotation et taille de la page {}",
                origin + 1
            );
        }
    }
}
