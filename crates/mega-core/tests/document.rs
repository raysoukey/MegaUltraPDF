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
