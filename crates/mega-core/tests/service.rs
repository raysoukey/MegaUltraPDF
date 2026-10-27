// SPDX-License-Identifier: GPL-3.0-or-later
//! Tests de l'acteur PDFium (`mega_core::service`).

use std::sync::Arc;
use std::time::Duration;

use mega_core::service::{PdfService, Priority, RenderRequest, ServiceHandle};
use mega_core::testing::sample_pdf;
use mega_core::{CoreError, Document, Rotation, SearchOptions};

fn request(doc: u32, page: u16, generation: u64, priority: Priority) -> RenderRequest {
    RenderRequest {
        doc,
        page,
        scale: 0.5,
        rotation: Rotation::None,
        generation,
        priority,
    }
}

fn open(handle: &ServiceHandle, pages: u16) -> u32 {
    handle
        .open_bytes(sample_pdf(pages, "service"), None)
        .unwrap()
        .id
}

#[test]
fn open_and_summary() {
    let handle = PdfService::spawn();
    let summary = handle.open_bytes(sample_pdf(3, "Résumé"), None).unwrap();
    assert_eq!(summary.page_count, 3);
    assert_eq!(summary.title.as_deref(), Some("Résumé"));
    assert_eq!(summary.pages.len(), 3);
    assert_eq!(handle.summary(summary.id).unwrap().id, summary.id);
}

#[test]
fn close_then_use_is_not_found() {
    let handle = PdfService::spawn();
    let id = open(&handle, 1);
    handle.close(id).unwrap();
    assert!(matches!(
        handle.summary(id),
        Err(CoreError::NotFound { id: missing }) if missing == id
    ));
    assert!(matches!(handle.close(id), Err(CoreError::NotFound { .. })));
}

#[test]
fn render_png_is_cached() {
    let handle = PdfService::spawn();
    let id = open(&handle, 1);
    let first = handle
        .render_png(request(id, 0, 0, Priority::Visible))
        .unwrap();
    let second = handle
        .render_png(request(id, 0, 0, Priority::Visible))
        .unwrap();
    assert_eq!(&first[..8], b"\x89PNG\r\n\x1a\n");
    assert!(
        Arc::ptr_eq(&first, &second),
        "le second rendu doit venir du cache"
    );
}

#[test]
fn set_generation_cancels_older_requests_but_keeps_cache() {
    let handle = PdfService::spawn();
    let id = open(&handle, 1);
    let first = handle
        .render_png(request(id, 0, 0, Priority::Visible))
        .unwrap();
    handle.set_generation(id, 3).unwrap();
    assert_eq!(handle.summary(id).unwrap().generation, 3);
    // Génération périmée : refusée sans travail.
    assert!(matches!(
        handle.render_png(request(id, 0, 2, Priority::Visible)),
        Err(CoreError::Cancelled)
    ));
    // Même échelle, génération courante : l'image vient toujours du cache (pas de rerendu).
    let again = handle
        .render_png(request(id, 0, 3, Priority::Visible))
        .unwrap();
    assert!(Arc::ptr_eq(&first, &again));
    // Une génération plus récente que celle connue de l'acteur fait avancer le compteur
    // (le frontend est l'autorité) et n'est jamais refusée.
    assert!(
        handle
            .render_png(request(id, 0, 5, Priority::Visible))
            .is_ok()
    );
    assert_eq!(handle.summary(id).unwrap().generation, 5);
    // `set_generation` ne recule jamais.
    assert_eq!(handle.set_generation(id, 1).unwrap(), 5);
}

#[test]
fn stale_generation_is_cancelled() {
    let handle = PdfService::spawn();
    let id = open(&handle, 1);
    handle.set_generation(id, 1).unwrap();
    assert!(matches!(
        handle.render_png(request(id, 0, 0, Priority::Visible)),
        Err(CoreError::Cancelled)
    ));
}

#[test]
fn render_after_close_is_not_found() {
    let handle = PdfService::spawn();
    let id = open(&handle, 1);
    handle.close(id).unwrap();
    assert!(matches!(
        handle.render_png(request(id, 0, 0, Priority::Visible)),
        Err(CoreError::NotFound { .. })
    ));
}

#[test]
fn render_out_of_range_page_is_reported() {
    let handle = PdfService::spawn();
    let id = open(&handle, 1);
    assert!(matches!(
        handle.render_png(request(id, 9, 0, Priority::Visible)),
        Err(CoreError::PageOutOfRange { .. })
    ));
}

#[test]
fn search_whole_document() {
    let handle = PdfService::spawn();
    let id = open(&handle, 3);
    let hits = handle.search(id, "Page", SearchOptions::default()).unwrap();
    assert_eq!(hits.len(), 3);
    assert_eq!(
        hits.iter().map(|h| h.page).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert!(handle.text_boxes(id, 2).unwrap().len() > 3);
    assert!(handle.outline(id).unwrap().is_empty());
    assert!(handle.links(id, 0).unwrap().is_empty());
    assert_eq!(handle.properties(id).unwrap().page_count, 3);
}

#[test]
fn handles_are_cloneable_across_threads() {
    let handle = PdfService::spawn();
    let id = open(&handle, 2);
    let workers: Vec<_> = (0..4)
        .map(|i| {
            let h = handle.clone();
            std::thread::spawn(move || {
                let png = h
                    .render_png(request(id, (i % 2) as u16, 0, Priority::Prefetch))
                    .unwrap();
                png.len()
            })
        })
        .collect();
    for w in workers {
        assert!(w.join().unwrap() > 100);
    }
}

#[test]
fn dropped_handles_stop_the_service() {
    let handle = PdfService::spawn();
    let id = open(&handle, 1);
    let clone = handle.clone();
    drop(handle);
    assert!(
        clone.summary(id).is_ok(),
        "le service vit tant qu'un handle existe"
    );
}

// ---- Lot 2 : éditions par l'acteur ----

#[test]
fn apply_invalidates_render_cache_and_reports_state() {
    use mega_core::EditCommand;
    let handle = PdfService::spawn();
    let id = open(&handle, 3);
    let before = handle
        .render_png(request(id, 0, 0, Priority::Visible))
        .unwrap();
    let (summary, state) = handle
        .apply(
            id,
            EditCommand::RotatePages {
                pages: vec![0],
                quarter_turns: 1,
            },
        )
        .unwrap();
    assert_eq!(summary.page_count, 3);
    assert!(state.modified && state.can_undo && !state.can_redo);
    assert_eq!(state.undo_label.as_deref(), Some("Pivoter 1 page"));
    let after = handle
        .render_png(request(id, 0, 0, Priority::Visible))
        .unwrap();
    assert!(
        !Arc::ptr_eq(&before, &after),
        "l'image en cache était périmée"
    );
    assert_eq!(handle.edit_state(id).unwrap(), state);
}

#[test]
fn undo_redo_through_service() {
    use mega_core::EditCommand;
    let handle = PdfService::spawn();
    let id = open(&handle, 3);
    handle
        .apply(id, EditCommand::DeletePages { pages: vec![1] })
        .unwrap();
    let (summary, state) = handle.undo(id).unwrap();
    assert_eq!(summary.page_count, 3);
    assert!(state.can_redo && !state.modified);
    let (summary, _) = handle.redo(id).unwrap();
    assert_eq!(summary.page_count, 2);
    assert!(matches!(handle.undo(99), Err(CoreError::NotFound { .. })));
}

#[test]
fn save_as_through_service_updates_path() {
    use mega_core::EditCommand;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sortie.pdf");
    let handle = PdfService::spawn();
    let id = open(&handle, 2);
    handle
        .apply(id, EditCommand::DeletePages { pages: vec![0] })
        .unwrap();
    let (summary, state) = handle.save_as(id, &path).unwrap();
    assert_eq!(summary.path.as_deref(), Some(path.to_str().unwrap()));
    assert!(!state.modified);
    assert_eq!(Document::open(&path, None).unwrap().page_count(), 1);
    // `save` sans chemin : InvalidOperation ; avec chemin : OK.
    let id2 = open(&handle, 1);
    assert!(matches!(
        handle.save(id2),
        Err(CoreError::InvalidOperation(_))
    ));
    assert!(handle.save(id).is_ok());
}

#[test]
fn extract_merge_split_through_service() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.pdf");
    let b = dir.path().join("b.pdf");
    std::fs::write(&a, sample_pdf(3, "a")).unwrap();
    std::fs::write(&b, mega_core::testing::outlined_pdf()).unwrap();
    let handle = PdfService::spawn();
    let id = handle.open_path(&a, None).unwrap().id;

    let out = dir.path().join("extrait.pdf");
    handle.extract_pages(id, vec![0, 2], &out).unwrap();
    assert_eq!(Document::open(&out, None).unwrap().page_count(), 2);

    let merged = handle.merge_files(vec![a.clone(), b.clone()]).unwrap();
    assert_eq!(merged.page_count, 6);
    assert!(
        merged.path.is_none(),
        "la fusion est un nouveau document non enregistré"
    );

    let files = handle
        .split_document(id, vec![vec![0, 1], vec![2]], dir.path(), "partie")
        .unwrap();
    assert_eq!(files.len(), 2);
    assert!(files[0].ends_with("partie-1.pdf") && files[1].ends_with("partie-2.pdf"));
    assert_eq!(Document::open(&files[1], None).unwrap().page_count(), 1);
}

// ---- Passe de correction après relecture (Lot 2) ----

/// I5 : un document fusionné n'existe pas sur disque : « modifié » tant qu'il n'est pas enregistré,
/// même après une édition annulée.
#[test]
fn merged_document_stays_modified_through_undo() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.pdf");
    let b = dir.path().join("b.pdf");
    std::fs::write(&a, sample_pdf(2, "a")).unwrap();
    std::fs::write(&b, sample_pdf(1, "b")).unwrap();
    let handle = PdfService::spawn();
    let summary = handle.merge_files(vec![a, b]).unwrap();
    assert_eq!(summary.page_count, 3);
    assert!(handle.edit_state(summary.id).unwrap().modified);
    let (_, state) = handle
        .apply(
            summary.id,
            mega_core::EditCommand::DeletePages { pages: vec![0] },
        )
        .unwrap();
    assert!(state.modified);
    let (_, state) = handle.undo(summary.id).unwrap();
    assert!(state.modified, "jamais enregistré : toujours modifié");
    let (_, state) = handle
        .save_as(summary.id, dir.path().join("fusion.pdf"))
        .unwrap();
    assert!(!state.modified);
}

// ---- Lot 3 : annotations ----

#[test]
fn annotations_request_and_cache_invalidation() {
    use mega_core::{AnnotationKind, Color, EditCommand, Geometry, NewAnnotation, Rect, Style};
    let handle = PdfService::spawn();
    let id = open(&handle, 1);
    assert!(handle.annotations(id, 0).unwrap().is_empty());
    assert!(matches!(
        handle.annotations(id, 4),
        Err(CoreError::PageOutOfRange { .. })
    ));
    let before = handle
        .render_png(request(id, 0, 0, Priority::Visible))
        .unwrap();
    let (_, state) = handle
        .apply(
            id,
            EditCommand::AddAnnotation {
                page: 0,
                annotation: NewAnnotation {
                    kind: AnnotationKind::Rect,
                    geometry: Geometry::Rect {
                        rect: Rect {
                            x: 100.0,
                            y: 500.0,
                            w: 200.0,
                            h: 100.0,
                        },
                    },
                    style: Style {
                        color: Color {
                            r: 229,
                            g: 57,
                            b: 53,
                        },
                        fill: None,
                        opacity: 1.0,
                        stroke_width: 2.0,
                        font_size: 14.0,
                    },
                    contents: String::new(),
                    author: String::new(),
                },
            },
        )
        .unwrap();
    assert!(state.modified && state.can_undo);
    assert_eq!(handle.annotations(id, 0).unwrap().len(), 1);
    let after = handle
        .render_png(request(id, 0, 0, Priority::Visible))
        .unwrap();
    assert_ne!(
        before, after,
        "le cache de rendu doit être invalidé par l'ajout d'une annotation"
    );
}

// ---- Lot 4 : formulaires ----

#[test]
fn form_requests() {
    use mega_core::{EditCommand, FieldValue, FormType};
    let handle = PdfService::spawn();
    let id = handle
        .open_bytes(mega_core::testing::form_pdf(), None)
        .unwrap()
        .id;
    let info = handle.form_info(id).unwrap();
    assert_eq!(info.form_type, FormType::AcroForm);
    assert_eq!(info.field_count, 8);
    assert_eq!(handle.form_fields(id, 0).unwrap().len(), 8);
    let before = handle
        .render_png(request(id, 0, 0, Priority::Visible))
        .unwrap();
    let (_, state) = handle
        .apply(
            id,
            EditCommand::SetFieldValue {
                page: 0,
                index: 0,
                value: FieldValue::Text {
                    text: "Dupont".to_string(),
                },
            },
        )
        .unwrap();
    assert!(state.modified);
    assert_eq!(
        handle.form_fields(id, 0).unwrap()[0].value.as_deref(),
        Some("Dupont")
    );
    let after = handle
        .render_png(request(id, 0, 0, Priority::Visible))
        .unwrap();
    assert_ne!(before, after, "cache de rendu invalidé");
}

// --- Lot 8 : OCR et signature par l'acteur -----------------------------------------------

#[cfg(feature = "ocr")]
#[test]
fn pages_with_text_through_the_service() {
    let handle = PdfService::spawn();
    let scanned = handle
        .open_bytes(mega_core::testing::scanned_pdf(), None)
        .unwrap();
    assert_eq!(handle.pages_with_text(scanned.id).unwrap(), vec![false; 3]);
    let text = handle.open_bytes(sample_pdf(2, "texte"), None).unwrap();
    assert_eq!(handle.pages_with_text(text.id).unwrap(), vec![true; 2]);
}

#[cfg(feature = "signature")]
#[test]
fn signatures_and_signing_through_the_service() {
    use mega_core::digital_signature::{SignOptions, SignatureStatus};
    let handle = PdfService::spawn();
    let summary = handle.open_bytes(sample_pdf(2, "signé"), None).unwrap();
    assert!(handle.signatures(summary.id).unwrap().is_empty());
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("signe.pdf");
    let p12 =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/test-signer.p12");
    let (signed, state) = handle
        .sign_document(summary.id, &out, &p12, "test", SignOptions::default())
        .unwrap();
    assert_eq!(signed.path.as_deref(), Some(out.to_string_lossy().as_ref()));
    assert!(!state.modified);
    let list = handle.signatures(summary.id).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].status, SignatureStatus::Intact);
    // Mauvais mot de passe : erreur typée, rien d'écrit.
    let other = dir.path().join("autre.pdf");
    let err = handle
        .sign_document(summary.id, &other, &p12, "faux", SignOptions::default())
        .unwrap_err();
    assert!(matches!(err, CoreError::WrongPassword), "{err:?}");
    assert!(!other.exists());
}
