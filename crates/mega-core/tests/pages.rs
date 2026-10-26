// SPDX-License-Identifier: GPL-3.0-or-later
//! Opérations de pages : rotation enregistrée, suppression, insertion, déplacement,
//! extraction, fusion, scission.

use mega_core::testing::{outlined_pdf, sample_pdf};
use mega_core::{CoreError, Document, PageSize, Rotation};

/// Texte d'une page sans espaces (« Page2 »), pour vérifier l'ordre des pages.
fn label(doc: &Document, page: u16) -> String {
    doc.text_boxes(page)
        .unwrap()
        .iter()
        .map(|b| b.ch)
        .filter(|c| !c.is_whitespace())
        .collect()
}

fn labels(doc: &Document) -> Vec<String> {
    (0..doc.page_count()).map(|i| label(doc, i)).collect()
}

fn three() -> Document {
    Document::from_bytes(sample_pdf(3, "trois"), None).unwrap()
}

#[test]
fn rotate_pages_is_persisted() {
    let mut doc = three();
    doc.rotate_pages(&[1], 1).unwrap();
    assert_eq!(doc.page_rotation(1).unwrap(), Rotation::Cw90);
    assert_eq!(doc.page_rotation(0).unwrap(), Rotation::None);
    let reloaded = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    assert_eq!(reloaded.page_rotation(1).unwrap(), Rotation::Cw90);
    // Un quart de tour enregistré échange largeur et hauteur de la page rendue.
    let r = reloaded.render_page(1, 0.5).unwrap();
    assert!(r.width_px > r.height_px);
    // Tourner de trois quarts de plus revient au nord ; les tours négatifs marchent aussi.
    doc.rotate_pages(&[1], 3).unwrap();
    assert_eq!(doc.page_rotation(1).unwrap(), Rotation::None);
    doc.rotate_pages(&[0], -1).unwrap();
    assert_eq!(doc.page_rotation(0).unwrap(), Rotation::Cw270);
}

#[test]
fn delete_pages_keeps_others_in_order() {
    let mut doc = three();
    doc.delete_pages(&[1]).unwrap();
    assert_eq!(doc.page_count(), 2);
    assert_eq!(labels(&doc), vec!["Page1", "Page3"]);
    assert!(matches!(
        doc.delete_pages(&[7]),
        Err(CoreError::PageOutOfRange { .. })
    ));
}

#[test]
fn delete_all_pages_is_refused() {
    let mut doc = three();
    assert!(matches!(
        doc.delete_pages(&[0, 1, 2]),
        Err(CoreError::InvalidOperation(_))
    ));
    assert_eq!(doc.page_count(), 3);
}

#[test]
fn insert_blank_page_has_requested_size() {
    let mut doc = three();
    let size = PageSize {
        width_pt: 300.0,
        height_pt: 400.0,
    };
    doc.insert_blank_page(1, size).unwrap();
    assert_eq!(doc.page_count(), 4);
    let s = doc.page_size(1).unwrap();
    assert!((s.width_pt - 300.0).abs() < 0.5 && (s.height_pt - 400.0).abs() < 0.5);
    assert!(doc.text_boxes(1).unwrap().is_empty());
    assert_eq!(labels(&doc), vec!["Page1", "", "Page2", "Page3"]);
    // Insertion en fin de document.
    doc.insert_blank_page(4, size).unwrap();
    assert_eq!(doc.page_count(), 5);
}

#[test]
fn insert_pages_from_another_document() {
    let mut doc = three();
    let other = Document::from_bytes(outlined_pdf(), None).unwrap();
    doc.insert_pages_from(&other, &[2, 0], 0).unwrap();
    assert_eq!(doc.page_count(), 5);
    // Les pages insérées gardent l'ordre demandé.
    assert_eq!(
        labels(&doc),
        vec!["Page3", "Page1", "Page1", "Page2", "Page3"]
    );
    assert_eq!(other.page_count(), 3, "la source n'est pas modifiée");
}

#[test]
fn move_pages_reorders() {
    let mut doc = three();
    doc.move_pages(&[2], 0).unwrap();
    assert_eq!(labels(&doc), vec!["Page3", "Page1", "Page2"]);
    // `to` est l'index d'insertion une fois les pages déplacées retirées.
    doc.move_pages(&[0], 2).unwrap();
    assert_eq!(labels(&doc), vec!["Page1", "Page2", "Page3"]);
    doc.move_pages(&[0, 1], 1).unwrap();
    assert_eq!(labels(&doc), vec!["Page3", "Page1", "Page2"]);
    assert!(matches!(
        doc.move_pages(&[0], 9),
        Err(CoreError::PageOutOfRange { .. })
    ));
}

#[test]
fn extract_pages_creates_a_new_document() {
    let doc = three();
    // L'ordre demandé est respecté (comme pour l'insertion depuis un autre document).
    let bytes = doc.extract_pages(&[2, 0]).unwrap();
    let extracted = Document::from_bytes(bytes, None).unwrap();
    assert_eq!(labels(&extracted), vec!["Page3", "Page1"]);
    assert!(doc.extract_pages(&[]).is_err());
    assert_eq!(doc.page_count(), 3);
}

#[test]
fn merge_concatenates_documents() {
    let a = three();
    let b = Document::from_bytes(outlined_pdf(), None).unwrap();
    let merged = Document::from_bytes(mega_core::pages::merge(&[&a, &b]).unwrap(), None).unwrap();
    assert_eq!(merged.page_count(), 6);
    assert_eq!(label(&merged, 3), "Page1");
}

#[test]
fn split_by_parts() {
    let doc = three();
    let parts = mega_core::pages::split(&doc, &[vec![0, 1], vec![2]]).unwrap();
    assert_eq!(parts.len(), 2);
    let first = Document::from_bytes(parts[0].clone(), None).unwrap();
    let second = Document::from_bytes(parts[1].clone(), None).unwrap();
    assert_eq!(labels(&first), vec!["Page1", "Page2"]);
    assert_eq!(labels(&second), vec!["Page3"]);
    assert!(mega_core::pages::split(&doc, &[]).is_err());
}

// ---- Passe de correction après relecture (Lot 2) ----

use mega_core::pages::{SplitMode, plan_split, split_to_dir};

/// I6 : la règle de scission vit dans le cœur (CLI et application l'appellent).
#[test]
fn plan_split_by_ranges_every_and_bookmarks() {
    let doc = Document::from_bytes(sample_pdf(5, "plan"), None).unwrap();
    assert_eq!(
        plan_split(
            &doc,
            &SplitMode::Ranges {
                spec: "1-2,4-".into()
            }
        )
        .unwrap(),
        vec![vec![0, 1], vec![3, 4]]
    );
    assert_eq!(
        plan_split(&doc, &SplitMode::Every { n: 2 }).unwrap(),
        vec![vec![0, 1], vec![2, 3], vec![4]]
    );
    // Signets de premier niveau en pages 1 et 3 → [1-2] et [3].
    let outlined = Document::from_bytes(outlined_pdf(), None).unwrap();
    assert_eq!(
        plan_split(&outlined, &SplitMode::Bookmarks).unwrap(),
        vec![vec![0, 1], vec![2]]
    );
}

#[test]
fn plan_split_rejects_bad_input() {
    let doc = Document::from_bytes(sample_pdf(5, "plan"), None).unwrap();
    assert!(matches!(
        plan_split(&doc, &SplitMode::Ranges { spec: "9".into() }),
        Err(CoreError::InvalidPageSpec(_))
    ));
    assert!(matches!(
        plan_split(&doc, &SplitMode::Every { n: 0 }),
        Err(CoreError::InvalidOperation(_))
    ));
    assert!(matches!(
        plan_split(&doc, &SplitMode::Bookmarks),
        Err(CoreError::InvalidOperation(_))
    ));
}

/// I3 : scinder n'écrase rien sans `overwrite`, et refuse un préfixe qui sort du dossier.
#[test]
fn split_to_dir_refuses_existing_outputs_unless_forced() {
    let doc = three();
    let dir = tempfile::tempdir().unwrap();
    let parts = vec![vec![0u16], vec![1, 2]];
    let files = split_to_dir(&doc, &parts, dir.path(), "part", false).unwrap();
    assert_eq!(files.len(), 2);
    assert!(dir.path().join("part-2.pdf").exists());
    // Deuxième passage : refus avant toute écriture, les fichiers existants sont intacts.
    std::fs::write(dir.path().join("part-1.pdf"), b"modifie").unwrap();
    let err = split_to_dir(&doc, &parts, dir.path(), "part", false).unwrap_err();
    assert!(matches!(err, CoreError::InvalidOperation(ref m) if m.contains("existe déjà")));
    assert_eq!(
        std::fs::read(dir.path().join("part-1.pdf")).unwrap(),
        b"modifie"
    );
    // Forcé : réécrit.
    split_to_dir(&doc, &parts, dir.path(), "part", true).unwrap();
    let rewritten = std::fs::read(dir.path().join("part-1.pdf")).unwrap();
    assert_eq!(
        Document::from_bytes(rewritten, None).unwrap().page_count(),
        1
    );
    for bad in ["../x", "a/b", "a\\b", ""] {
        assert!(
            matches!(
                split_to_dir(&doc, &parts, dir.path(), bad, true),
                Err(CoreError::InvalidOperation(_))
            ),
            "préfixe refusé : {bad:?}"
        );
    }
}
