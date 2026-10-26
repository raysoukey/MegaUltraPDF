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

#[test]
fn open_encrypted_with_wrong_password_is_wrong_password() {
    let bytes = std::fs::read(fixture("encrypted.pdf")).unwrap();
    assert!(matches!(
        Document::from_bytes(bytes, Some("faux")),
        Err(CoreError::WrongPassword)
    ));
}

#[test]
fn open_rejects_more_than_u16_max_pages() {
    let bytes = mega_core::testing::many_pages_pdf(65_536);
    assert!(matches!(
        Document::from_bytes(bytes, None),
        Err(CoreError::InvalidPdf(msg)) if msg.contains("65 535")
    ));
    // Juste sous la limite : accepté, et le compte est exact.
    let ok = Document::from_bytes(mega_core::testing::many_pages_pdf(65_535), None).unwrap();
    assert_eq!(ok.page_count(), 65_535);
}

// ---- Lot 1 : texte, recherche, signets, liens, propriétés, rotation ----

#[test]
fn text_boxes_read_page_label() {
    let doc = Document::from_bytes(sample_pdf(3, "x"), None).unwrap();
    let boxes = doc.text_boxes(1).unwrap();
    let text: String = boxes
        .iter()
        .map(|b| b.ch)
        .filter(|c| !c.is_whitespace())
        .collect();
    assert!(text.starts_with("Page2"), "{text}");
    assert!(boxes.iter().all(|b| b.rect.h > 0.0 && b.rect.w >= 0.0));
}

#[test]
fn text_boxes_empty_on_blank_page() {
    let doc = Document::from_bytes(mega_core::testing::blank_pdf(200.0, 200.0), None).unwrap();
    assert!(doc.text_boxes(0).unwrap().is_empty());
    let hits = doc
        .search_page(0, "rien", &mega_core::SearchOptions::default())
        .unwrap();
    assert!(hits.is_empty());
}

#[test]
fn search_finds_each_page_once() {
    let doc = Document::from_bytes(sample_pdf(3, "x"), None).unwrap();
    let opts = mega_core::SearchOptions::default();
    for page in 0..3 {
        let hits = doc.search_page(page, "Page", &opts).unwrap();
        assert_eq!(hits.len(), 1, "page {page}");
        assert_eq!(hits[0].page, page);
        assert_eq!(hits[0].rects.len(), 1);
    }
}

#[test]
fn search_respects_case() {
    let doc = Document::from_bytes(sample_pdf(1, "x"), None).unwrap();
    let strict = mega_core::SearchOptions {
        match_case: true,
        whole_word: false,
    };
    assert!(doc.search_page(0, "page", &strict).unwrap().is_empty());
    assert_eq!(
        doc.search_page(0, "page", &mega_core::SearchOptions::default())
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn outline_tree_has_child() {
    let doc = Document::from_bytes(mega_core::testing::outlined_pdf(), None).unwrap();
    let outline = doc.outline().unwrap();
    assert_eq!(outline.len(), 2);
    assert_eq!(outline[0].title, "Chapitre 1");
    assert_eq!(outline[0].page, Some(0));
    assert_eq!(outline[0].children.len(), 1);
    assert_eq!(outline[0].children[0].title, "Section 1.1");
    assert_eq!(outline[0].children[0].page, Some(1));
    assert_eq!(outline[1].page, Some(2));
    assert!(
        Document::from_bytes(sample_pdf(1, "x"), None)
            .unwrap()
            .outline()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn links_internal_and_uri() {
    use mega_core::LinkTarget;
    let doc = Document::from_bytes(mega_core::testing::outlined_pdf(), None).unwrap();
    let links = doc.links(0).unwrap();
    assert_eq!(links.len(), 2);
    assert!(
        links
            .iter()
            .any(|l| matches!(l.target, LinkTarget::Page(2)))
    );
    assert!(
        links
            .iter()
            .any(|l| matches!(&l.target, LinkTarget::Uri(u) if u == "https://example.org"))
    );
    assert!(links.iter().all(|l| l.rect.w > 0.0 && l.rect.h > 0.0));
    assert!(doc.links(1).unwrap().is_empty());
}

#[test]
fn properties_expose_title_and_version() {
    let doc = Document::from_bytes(sample_pdf(2, "Propriétés"), None).unwrap();
    let p = doc.properties().unwrap();
    assert_eq!(p.title.as_deref(), Some("Propriétés"));
    assert!(p.pdf_version.starts_with("1."), "{}", p.pdf_version);
    assert_eq!(p.page_count, 2);
    assert_eq!(p.producer.as_deref(), Some("MegaUltraPDF tests"));
}

#[test]
fn render_rotated_swaps_dimensions() {
    use mega_core::Rotation;
    let doc = Document::from_bytes(sample_pdf(1, "x"), None).unwrap();
    let upright = doc.render_page_with(0, 0.5, Rotation::None).unwrap();
    let turned = doc.render_page_with(0, 0.5, Rotation::Cw90).unwrap();
    assert!(upright.height_px > upright.width_px);
    assert_eq!(
        (turned.width_px, turned.height_px),
        (upright.height_px, upright.width_px)
    );
}

#[test]
fn outline_terminates_on_cycle_and_depth() {
    // Un PDF hostile : /Next qui boucle sur lui-même, et un enfant qui est son propre enfant.
    let doc = Document::from_bytes(mega_core::testing::cyclic_outline_pdf(), None).unwrap();
    let outline = doc.outline().unwrap();
    assert!(!outline.is_empty());
    assert!(outline.len() <= mega_core::outline::MAX_OUTLINE_NODES);
    fn depth(n: &mega_core::OutlineNode) -> usize {
        1 + n.children.iter().map(depth).max().unwrap_or(0)
    }
    assert!(outline.iter().map(depth).max().unwrap_or(0) <= mega_core::outline::MAX_OUTLINE_DEPTH);
}
