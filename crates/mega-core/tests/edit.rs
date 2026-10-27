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

// ---- Passe de correction après relecture (Lot 2) ----

fn fixture(name: &str) -> String {
    format!("{}/../../fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

/// C1 : les instantanés d'un PDF chiffré restent chiffrés ; annuler, rétablir et déplacer
/// doivent recharger avec le mot de passe d'ouverture.
#[test]
fn encrypted_document_supports_undo_redo_and_move() {
    let mut doc = Document::open(fixture("encrypted.pdf"), Some("secret")).unwrap();
    assert_eq!(doc.page_count(), 3);
    doc.apply(&delete(0)).unwrap();
    assert_eq!(doc.page_count(), 2);
    doc.undo().unwrap();
    assert_eq!(doc.page_count(), 3);
    assert!(doc.can_redo());
    doc.redo().unwrap();
    assert_eq!(doc.page_count(), 2);
    doc.undo().unwrap();
    doc.apply(&EditCommand::MovePages {
        pages: vec![0],
        to: 2,
    })
    .unwrap();
    assert_eq!(doc.page_count(), 3);
    assert!(doc.can_undo());
}

/// C1 bis : si le rechargement échoue, l'étape n'est pas perdue.
struct Halfway;

impl mega_core::Command for Halfway {
    fn label(&self) -> String {
        "à mi-chemin".to_string()
    }
    fn apply(&self, doc: &mut Document) -> mega_core::CoreResult<()> {
        // Modifie réellement le document, puis échoue : le contrat exige qu'il soit restauré.
        doc.rotate_pages(&[0], 1)?;
        Err(CoreError::InvalidOperation("panne simulée".to_string()))
    }
}

/// I1 : une commande qui échoue après avoir modifié le document est entièrement annulée.
#[test]
fn failed_command_midway_restores_document() {
    let mut doc = three();
    assert!(doc.apply(&Halfway).is_err());
    assert_eq!(doc.page_rotation(0).unwrap(), mega_core::Rotation::None);
    assert!(!doc.can_undo());
    assert!(!doc.is_modified());
}

/// M7 : échec **après** l'écriture du temporaire (la cible est un dossier) : original intact,
/// aucun fichier temporaire orphelin.
#[test]
fn save_as_to_directory_path_leaves_original_intact_and_no_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("original.pdf");
    std::fs::write(&original, sample_pdf(2, "intact")).unwrap();
    let before = std::fs::read(&original).unwrap();
    let mut doc = Document::open(&original, None).unwrap();
    doc.apply(&delete(0)).unwrap();
    let target_dir = dir.path().join("cible.pdf");
    std::fs::create_dir(&target_dir).unwrap();
    assert!(doc.save_as(&target_dir).is_err());
    assert_eq!(std::fs::read(&original).unwrap(), before);
    assert!(target_dir.is_dir(), "le dossier n'a pas été remplacé");
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "fichiers temporaires orphelins : {leftovers:?}"
    );
    assert!(doc.is_modified());
}

/// I2 : enregistrer conserve le mode du fichier (POSIX).
#[cfg(unix)]
#[test]
fn save_preserves_file_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("partage.pdf");
    std::fs::write(&path, sample_pdf(2, "perm")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let mut doc = Document::open(&path, None).unwrap();
    doc.apply(&delete(0)).unwrap();
    doc.save().unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o644);
}

/// I5 : un document qui n'a jamais été enregistré (fusion) reste « modifié » même après
/// une édition annulée.
#[test]
fn never_saved_document_stays_modified_through_undo() {
    let mut doc = three();
    doc.mark_never_saved();
    assert!(doc.is_modified());
    doc.apply(&delete(0)).unwrap();
    doc.undo().unwrap();
    assert!(doc.is_modified());
    let dir = tempfile::tempdir().unwrap();
    doc.save_as(dir.path().join("fusion.pdf")).unwrap();
    assert!(!doc.is_modified());
}

// ---- Lot 3 : annotations ----

#[test]
fn annotation_commands_undo_and_labels() {
    use mega_core::{AnnotationKind, Color, Geometry, NewAnnotation, Rect, Style};
    let mut doc = three();
    let style = Style {
        color: Color {
            r: 229,
            g: 57,
            b: 53,
        },
        fill: None,
        opacity: 1.0,
        stroke_width: 2.0,
        font_size: 14.0,
    };
    let rect = NewAnnotation {
        kind: AnnotationKind::Rect,
        geometry: Geometry::Rect {
            rect: Rect {
                x: 100.0,
                y: 500.0,
                w: 200.0,
                h: 100.0,
            },
        },
        style,
        contents: String::new(),
        author: "Moi".to_string(),
    };
    doc.apply(&EditCommand::AddAnnotation {
        page: 0,
        annotation: rect.clone(),
    })
    .unwrap();
    assert_eq!(doc.annotations(0).unwrap().len(), 1);
    assert_eq!(doc.undo_label().as_deref(), Some("Ajouter : Rectangle"));
    assert!(doc.is_modified());
    doc.undo().unwrap();
    assert!(doc.annotations(0).unwrap().is_empty());
    doc.redo().unwrap();
    assert_eq!(doc.annotations(0).unwrap().len(), 1);

    doc.apply(&EditCommand::MoveAnnotation {
        page: 0,
        index: 0,
        rect: Rect {
            x: 10.0,
            y: 10.0,
            w: 100.0,
            h: 50.0,
        },
    })
    .unwrap();
    let a = &doc.annotations(0).unwrap()[0];
    assert!((a.rect.x - 10.0).abs() < 1.5, "{:?}", a.rect);
    assert_eq!(doc.undo_label().as_deref(), Some("Déplacer : Rectangle"));

    doc.apply(&EditCommand::UpdateAnnotation {
        page: 0,
        index: 0,
        patch: mega_core::AnnotationPatch {
            color: Some(Color { r: 0, g: 0, b: 255 }),
            ..Default::default()
        },
    })
    .unwrap();
    let anns = doc.annotations(0).unwrap();
    assert_eq!(anns.len(), 1);
    assert_eq!(anns[0].color, Some(Color { r: 0, g: 0, b: 255 }));
    assert_eq!(doc.undo_label().as_deref(), Some("Modifier : Rectangle"));

    doc.apply(&EditCommand::DeleteAnnotation { page: 0, index: 0 })
        .unwrap();
    assert!(doc.annotations(0).unwrap().is_empty());
    assert_eq!(doc.undo_label().as_deref(), Some("Supprimer : Rectangle"));
    // Supprimer un index absent : refusé, document et historique inchangés.
    assert!(
        doc.apply(&EditCommand::DeleteAnnotation { page: 0, index: 7 })
            .is_err()
    );
    assert_eq!(doc.undo_label().as_deref(), Some("Supprimer : Rectangle"));
}

// ---- Lot 4 : formulaires ----

#[test]
fn form_commands_are_undoable() {
    use mega_core::FieldValue;
    let mut doc = Document::from_bytes(mega_core::testing::form_pdf(), None).unwrap();
    doc.apply(&EditCommand::SetFieldValue {
        page: 0,
        index: 0,
        value: FieldValue::Text {
            text: "Dupont".to_string(),
        },
    })
    .unwrap();
    assert_eq!(doc.undo_label().as_deref(), Some("Remplir : nom"));
    assert_eq!(
        doc.form_fields(0).unwrap()[0].value.as_deref(),
        Some("Dupont")
    );
    doc.undo().unwrap();
    assert_eq!(doc.form_fields(0).unwrap()[0].value.as_deref(), Some(""));
    doc.redo().unwrap();
    doc.apply(&EditCommand::FlattenForm).unwrap();
    assert_eq!(doc.undo_label().as_deref(), Some("Aplatir le formulaire"));
    assert!(doc.form_fields(0).unwrap().is_empty());
    doc.undo().unwrap();
    assert_eq!(doc.form_fields(0).unwrap().len(), 8);
    assert_eq!(
        doc.form_fields(0).unwrap()[0].value.as_deref(),
        Some("Dupont")
    );
}

// --- Octets intacts (Lot 8) : un document non modifié s'enregistre tel quel, ce qui garde
// valides ses signatures numériques (PDFium réécrirait tout le fichier).

fn rotate_first() -> EditCommand {
    EditCommand::RotatePages {
        pages: vec![0],
        quarter_turns: 1,
    }
}

#[test]
fn untouched_save_writes_the_loaded_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = sample_pdf(3, "intact");
    let mut doc = Document::from_bytes(bytes.clone(), None).unwrap();
    let path = dir.path().join("intact.pdf");
    doc.save_as(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    // Et encore après un premier enregistrement, tant que rien n'a changé.
    let again = dir.path().join("encore.pdf");
    doc.save_as(&again).unwrap();
    assert_eq!(std::fs::read(&again).unwrap(), bytes);
}

#[test]
fn save_after_an_edit_rewrites_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = sample_pdf(3, "édité");
    let mut doc = Document::from_bytes(bytes.clone(), None).unwrap();
    doc.apply(&rotate_first()).unwrap();
    let path = dir.path().join("edite.pdf");
    doc.save_as(&path).unwrap();
    assert_ne!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(Document::open(&path, None).unwrap().page_count(), 3);
}

#[test]
fn undo_back_to_the_saved_state_writes_the_file_bytes() {
    // Revenir par « annuler » à l'état du fichier rend le document de nouveau intact : un
    // PDF signé, modifié puis remis en l'état, garde sa signature à l'enregistrement.
    let dir = tempfile::tempdir().unwrap();
    let bytes = sample_pdf(2, "annulé");
    let mut doc = Document::from_bytes(bytes.clone(), None).unwrap();
    doc.apply(&rotate_first()).unwrap();
    doc.undo().unwrap();
    let path = dir.path().join("annule.pdf");
    doc.save_as(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    // Rétablir : de nouveau modifié, donc réécrit.
    doc.redo().unwrap();
    let again = dir.path().join("retabli.pdf");
    doc.save_as(&again).unwrap();
    assert_ne!(std::fs::read(&again).unwrap(), bytes);
}

#[test]
fn file_bytes_survive_an_edit() {
    // La vérification des signatures lit le fichier, pas une réécriture PDFium.
    let bytes = sample_pdf(1, "fichier");
    let mut doc = Document::from_bytes(bytes.clone(), None).unwrap();
    doc.apply(&rotate_first()).unwrap();
    assert_eq!(doc.file_bytes(), bytes.as_slice());
}

#[test]
fn direct_mutation_then_save_writes_the_change() {
    // Chemin du CLI (`mega rotate`, `mega redact`, `mega optimize`…) : appel direct, sans
    // historique, puis `save_as`. Le fichier écrit doit contenir la modification.
    let dir = tempfile::tempdir().unwrap();
    let bytes = sample_pdf(2, "direct");
    let mut doc = Document::from_bytes(bytes.clone(), None).unwrap();
    doc.rotate_pages(&[0], 1).unwrap();
    let path = dir.path().join("direct.pdf");
    doc.save_as(&path).unwrap();
    assert_ne!(std::fs::read(&path).unwrap(), bytes);
    let reopened = Document::open(&path, None).unwrap();
    let size = reopened.page_size(0).unwrap();
    assert!(size.width_pt > size.height_pt, "page 1 tournée en paysage");
}
