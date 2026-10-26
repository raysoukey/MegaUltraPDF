// SPDX-License-Identifier: GPL-3.0-or-later
//! Commandes d'édition, historique (annuler / rétablir), enregistrement atomique.

use mega_core::edit::{EditCommand, HistoryLimits};
use mega_core::testing::sample_pdf;
use mega_core::{CoreError, Document, PageSize};

fn labels(doc: &Document) -> Vec<String> {
    (0..doc.page_count())
        .map(|i| {
            doc.text_boxes(i)
                .unwrap()
                .iter()
                .map(|b| b.ch)
                .filter(|c| !c.is_whitespace())
                .collect()
        })
        .collect()
}

fn three() -> Document {
    Document::from_bytes(sample_pdf(3, "édition"), None).unwrap()
}

fn delete(page: u16) -> EditCommand {
    EditCommand::DeletePages { pages: vec![page] }
}

#[test]
fn apply_then_undo_restores_pages() {
    let mut doc = three();
    assert!(!doc.can_undo() && !doc.can_redo() && !doc.is_modified());
    doc.apply(&delete(1)).unwrap();
    assert_eq!(labels(&doc), vec!["Page1", "Page3"]);
    assert!(doc.can_undo() && doc.is_modified());
    assert_eq!(doc.undo_label().as_deref(), Some("Supprimer 1 page"));
    doc.undo().unwrap();
    assert_eq!(labels(&doc), vec!["Page1", "Page2", "Page3"]);
    assert!(!doc.can_undo() && doc.can_redo());
}

#[test]
fn redo_reapplies_and_new_edit_clears_redo() {
    let mut doc = three();
    doc.apply(&delete(0)).unwrap();
    doc.undo().unwrap();
    doc.redo().unwrap();
    assert_eq!(labels(&doc), vec!["Page2", "Page3"]);
    assert!(!doc.can_redo());
    doc.undo().unwrap();
    doc.apply(&EditCommand::RotatePages {
        pages: vec![0],
        quarter_turns: 1,
    })
    .unwrap();
    assert!(!doc.can_redo(), "une nouvelle édition efface le rétablir");
    assert_eq!(doc.undo_label().as_deref(), Some("Pivoter 1 page"));
    assert!(matches!(doc.redo(), Err(CoreError::InvalidOperation(_))));
}

#[test]
fn failed_command_leaves_document_and_history_untouched() {
    let mut doc = three();
    assert!(doc.apply(&delete(9)).is_err());
    assert!(!doc.can_undo() && !doc.is_modified());
    assert_eq!(doc.page_count(), 3);
}

#[test]
fn history_is_bounded() {
    let mut doc = Document::from_bytes(sample_pdf(30, "long"), None)
        .unwrap()
        .with_history_limits(HistoryLimits {
            max_steps: 5,
            max_bytes: usize::MAX,
        });
    for _ in 0..8 {
        doc.apply(&delete(0)).unwrap();
    }
    assert_eq!(doc.page_count(), 22);
    let mut undone = 0;
    while doc.can_undo() {
        doc.undo().unwrap();
        undone += 1;
    }
    assert_eq!(undone, 5);
    assert_eq!(doc.page_count(), 27);
}

#[test]
fn history_byte_budget_evicts_oldest() {
    let mut doc = three().with_history_limits(HistoryLimits {
        max_steps: 100,
        max_bytes: 1, // aucun instantané ne tient : seul le dernier est gardé
    });
    doc.apply(&delete(0)).unwrap();
    doc.apply(&delete(0)).unwrap();
    assert!(doc.can_undo());
    doc.undo().unwrap();
    assert!(!doc.can_undo());
    assert_eq!(doc.page_count(), 2);
}

#[test]
fn is_modified_lifecycle_and_undo_after_save() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc.pdf");
    std::fs::write(&path, sample_pdf(3, "fichier")).unwrap();
    let mut doc = Document::open(&path, None).unwrap();
    doc.apply(&delete(2)).unwrap();
    assert!(doc.is_modified());
    doc.save().unwrap();
    assert!(!doc.is_modified());
    assert_eq!(Document::open(&path, None).unwrap().page_count(), 2);
    // Annuler reste possible après l'enregistrement, et marque le document modifié.
    doc.undo().unwrap();
    assert!(doc.is_modified() && doc.page_count() == 3);
    doc.redo().unwrap();
    assert!(
        !doc.is_modified(),
        "revenir à l'état enregistré n'est pas une modification"
    );
}

#[test]
fn save_as_is_atomic_and_reloadable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("copie.pdf");
    let mut doc = three();
    assert!(
        matches!(doc.save(), Err(CoreError::InvalidOperation(_))),
        "pas de chemin"
    );
    doc.apply(&delete(0)).unwrap();
    doc.save_as(&path).unwrap();
    assert_eq!(doc.path(), Some(path.as_path()));
    assert!(!doc.is_modified());
    assert_eq!(
        labels(&Document::open(&path, None).unwrap()),
        vec!["Page2", "Page3"]
    );
    // Aucun fichier temporaire ne traîne.
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name() != "copie.pdf")
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn save_as_failure_leaves_original_intact() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("original.pdf");
    std::fs::write(&original, sample_pdf(3, "intact")).unwrap();
    let before = std::fs::read(&original).unwrap();
    let mut doc = Document::open(&original, None).unwrap();
    doc.apply(&delete(0)).unwrap();
    // Cible impossible : un dossier inexistant.
    let bad = dir.path().join("inexistant").join("x.pdf");
    assert!(matches!(doc.save_as(&bad), Err(CoreError::Io(_))));
    assert_eq!(std::fs::read(&original).unwrap(), before);
    assert_eq!(
        doc.path(),
        Some(original.as_path()),
        "le chemin n'a pas changé"
    );
    assert!(doc.is_modified());
}

#[test]
fn insert_blank_and_from_file_commands() {
    let dir = tempfile::tempdir().unwrap();
    let other = dir.path().join("autre.pdf");
    std::fs::write(&other, mega_core::testing::outlined_pdf()).unwrap();
    let mut doc = three();
    doc.apply(&EditCommand::InsertBlankPage {
        at: 1,
        size: PageSize {
            width_pt: 200.0,
            height_pt: 100.0,
        },
    })
    .unwrap();
    doc.apply(&EditCommand::InsertPagesFromFile {
        path: other.display().to_string(),
        pages: Some(vec![2]),
        at: 0,
    })
    .unwrap();
    assert_eq!(labels(&doc), vec!["Page3", "Page1", "", "Page2", "Page3"]);
    doc.apply(&EditCommand::MovePages {
        pages: vec![0],
        to: 4,
    })
    .unwrap();
    assert_eq!(labels(&doc), vec!["Page1", "", "Page2", "Page3", "Page3"]);
    assert_eq!(doc.undo_label().as_deref(), Some("Déplacer 1 page"));
}

#[test]
fn edit_state_summarises_history() {
    let mut doc = three();
    doc.apply(&delete(0)).unwrap();
    let state = doc.edit_state();
    assert!(state.modified && state.can_undo && !state.can_redo);
    assert_eq!(state.undo_label.as_deref(), Some("Supprimer 1 page"));
    assert_eq!(state.redo_label, None);
}
