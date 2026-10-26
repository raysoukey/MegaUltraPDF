// SPDX-License-Identifier: GPL-3.0-or-later
//! Tests d'intégration de `mega-core` : ouverture, métadonnées, rendu, erreurs.
//!
//! \[Apprentissage\] Les fichiers de `tests/` sont compilés comme des crates séparés qui
//! n'ont accès qu'à l'API publique, exactement comme un utilisateur de la bibliothèque.

use mega_core::{CoreError, Document, testing::sample_pdf};

fn fixture(name: &str) -> String {
    format!("{}/../../fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn opens_bytes_and_reads_metadata() {
    let doc = Document::from_bytes(sample_pdf(3, "Exemple"), None).unwrap();
    assert_eq!(doc.page_count(), 3);
    assert_eq!(doc.title().as_deref(), Some("Exemple"));
    let s = doc.page_size(0).unwrap();
    assert!((s.width_pt - 595.0).abs() < 1.0 && (s.height_pt - 842.0).abs() < 1.0);
}

#[test]
fn open_rejects_non_pdf_bytes() {
    assert!(matches!(
        Document::from_bytes(b"pas un pdf".to_vec(), None),
        Err(CoreError::InvalidPdf(_))
    ));
}

#[test]
fn open_reports_missing_file() {
    assert!(matches!(
        Document::open("n/existe/pas.pdf", None),
        Err(CoreError::Io(_))
    ));
}

#[test]
fn render_out_of_range_page_fails() {
    let doc = Document::from_bytes(sample_pdf(1, "x"), None).unwrap();
    assert!(matches!(
        doc.render_page(5, 1.0),
        Err(CoreError::PageOutOfRange { index: 5, count: 1 })
    ));
}

#[test]
fn render_scale_controls_pixel_size() {
    let doc = Document::from_bytes(sample_pdf(1, "x"), None).unwrap();
    let size = doc.page_size(0).unwrap();
    let expected = (
        (size.width_pt * 2.0).round() as u32,
        (size.height_pt * 2.0).round() as u32,
    );
    let r = doc.render_page(0, 2.0).unwrap();
    assert_eq!((r.width_px, r.height_px), expected);
    assert_eq!(r.rgba.len(), (expected.0 * expected.1 * 4) as usize);
    let png = r.to_png().unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn roundtrip_to_bytes_keeps_pages() {
    let doc = Document::from_bytes(sample_pdf(2, "x"), None).unwrap();
    let again = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    assert_eq!(again.page_count(), 2);
}

#[test]
fn open_encrypted_without_password_is_password_required() {
    let bytes = std::fs::read(fixture("encrypted.pdf")).unwrap();
    assert!(matches!(
        Document::from_bytes(bytes.clone(), None),
        Err(CoreError::PasswordRequired)
    ));
    assert_eq!(
        Document::from_bytes(bytes, Some("secret"))
            .unwrap()
            .page_count(),
        3
    );
}

#[test]
fn title_with_non_ascii_characters_roundtrips() {
    let doc = Document::from_bytes(sample_pdf(1, "Résumé — été"), None).unwrap();
    assert_eq!(doc.title().as_deref(), Some("Résumé — été"));
}

#[test]
fn io_error_message_mentions_the_cause_exactly_once() {
    let err = Document::open("n/existe/pas.pdf", None).unwrap_err();
    let full = mega_core::error::display_chain(&err);
    assert!(full.starts_with("Erreur d'accès au fichier"), "{full}");
    assert_eq!(full.matches("os error").count(), 1, "{full}");
}

#[test]
fn render_refuses_absurd_pixel_counts() {
    // Page de 14 400 × 14 400 pt (maximum du format) : 207 Mpx à l'échelle 1.
    let doc =
        Document::from_bytes(mega_core::testing::blank_pdf(14_400.0, 14_400.0), None).unwrap();
    assert!(matches!(
        doc.render_page(0, 1.0),
        Err(CoreError::RenderTooLarge { .. })
    ));
    // Une page A4 à l'échelle 8 (≈ 32 Mpx) reste acceptée.
    let a4 = Document::from_bytes(sample_pdf(1, "x"), None).unwrap();
    assert!(a4.render_page(0, 8.0).is_ok());
}

#[test]
fn concurrent_first_use_is_safe() {
    // Régression macOS arm64 (CI du 2026-10-26) : plusieurs threads qui initialisaient
    // PDFium en même temps au premier appel faisaient planter le processus (SIGTRAP).
    // L'initialisation doit être strictement unique, les autres threads attendent.
    use std::sync::{Arc, Barrier};
    // Deuxième régression (CI Linux/macOS) : l'erreur du dernier chargement est globale
    // dans PDFium ; des chargements valides et invalides entrelacés transformaient
    // « fichier invalide » en « erreur inconnue ». Chaque thread fait donc les deux.
    let threads = 8;
    let barrier = Arc::new(Barrier::new(threads));
    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                for _ in 0..5 {
                    let doc = Document::from_bytes(sample_pdf(1, "course"), None).unwrap();
                    assert!(doc.render_page(0, 1.0).unwrap().width_px > 0);
                    assert!(matches!(
                        Document::from_bytes(b"pas un pdf".to_vec(), None),
                        Err(CoreError::InvalidPdf(_))
                    ));
                }
            })
        })
        .collect();
    for h in handles {
        h.join().expect("un thread a échoué");
    }
}
