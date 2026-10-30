// SPDX-License-Identifier: GPL-3.0-or-later
//! Robustesse face aux fichiers malformés ou hostiles (Lot 13, audit R1-R9).
//!
//! Chaque fichier de `fixtures/robustesse/` (produit par `cargo xtask gen-fixtures`) passe par
//! toute la chaîne d'un utilisateur : ouvrir, rendre, lire le texte, chercher, éditer
//! (filigrane, caviardage), optimiser, enregistrer, rouvrir. Un fichier a le droit d'être
//! refusé, **avec une erreur typée** ; il n'a jamais le droit de faire paniquer, figer ou
//! fermer l'application.

use mega_core::edit::EditCommand;
use mega_core::optimize::OptimizeOptions;
use mega_core::overlay::Watermark;
use mega_core::Color;
use mega_core::{CoreError, Document, Rect, SearchOptions};

fn corpus(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/../../fixtures/robustesse/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path} : {e}"))
}

/// Toute la chaîne d'un utilisateur sur un document ouvert. Chaque étape peut échouer avec
/// une erreur typée ; rend le texte de la première page relu après enregistrement.
fn exercise(mut doc: Document) -> String {
    let dir = tempfile::tempdir().unwrap();
    let _ = doc.page_count();
    let _ = doc.render_page(0, 1.0);
    let _ = doc.text_boxes(0);
    let _ = doc.page_text(0);
    let _ = doc.search_page(0, "e", &SearchOptions::default());
    let _ = doc.properties();
    let _ = doc.signatures();
    let _ = doc.apply(&EditCommand::Watermark {
        spec: Watermark {
            text: "Robuste".to_string(),
            size: 40.0,
            opacity: 0.3,
            angle_deg: 45.0,
            color: Color { r: 200, g: 0, b: 0 },
            pages: None,
        },
    });
    let _ = doc.apply(&EditCommand::Redact {
        // Hors du texte (x = 72) : le texte doit survivre jusqu'à la relecture.
        page: 0,
        rect: Rect {
            x: 350.0, y: 50.0, w: 200.0, h: 200.0
        },
    });
    let _ = doc.optimize(&OptimizeOptions::default());
    let path = dir.path().join("sortie.pdf");
    doc.save_as(&path)
        .expect("enregistrement d'un document ouvert");
    let reopened = Document::open(&path, None).expect("relecture de notre propre sortie");
    reopened.page_text(0).unwrap_or_default()
}

/// Ouvre `name` : un refus doit être une erreur de format, jamais une panique.
fn open_or_refuse(name: &str) -> Option<Document> {
    match Document::from_bytes(corpus(name), None) {
        Ok(doc) => Some(doc),
        Err(CoreError::InvalidPdf(_)) => None,
        Err(other) => panic!("{name} : refus inattendu {other:?}"),
    }
}

#[test]
fn an_empty_file_is_refused_as_invalid() {
    assert!(open_or_refuse("vide.pdf").is_none());
}

#[test]
fn a_truncated_file_is_refused_or_repaired() {
    if let Some(doc) = open_or_refuse("tronque.pdf") {
        exercise(doc);
    }
}

#[test]
fn junk_before_the_header_is_tolerated() {
    let doc = open_or_refuse("prefixe-parasite.pdf").expect("PDFium tolère 1 Ko de préfixe");
    assert!(exercise(doc).contains("Prefixe"));
}

#[test]
fn a_shifted_xref_is_repaired() {
    let doc = open_or_refuse("xref-decalee.pdf").expect("table reconstruite");
    assert!(exercise(doc).contains("Xref decalee"));
}

#[test]
fn a_missing_endobj_is_repaired() {
    let doc = open_or_refuse("sans-endobj.pdf").expect("objet relu sans endobj");
    assert!(exercise(doc).contains("Sans endobj"));
}

#[test]
fn a_pdf_2_0_header_opens() {
    let doc = open_or_refuse("pdf-2-0.pdf").expect("PDF 2.0");
    assert!(exercise(doc).contains("PDF 2.0"));
}

#[test]
fn the_largest_page_size_renders_within_the_pixel_cap() {
    let doc = open_or_refuse("page-immense.pdf").expect("page immense");
    // 14 400 pt à l'échelle 1 = 207 Mpx : refusé proprement, l'échelle réduite passe.
    assert!(matches!(
        doc.render_page(0, 1.0),
        Err(CoreError::RenderTooLarge { .. })
    ));
    assert!(doc.render_page(0, 0.25).is_ok());
    assert!(exercise(doc).contains("Immense"));
}

#[test]
fn a_hybrid_reference_file_keeps_its_font_through_a_save() {
    let doc = open_or_refuse("xref-hybride.pdf").expect("fichier hybride");
    assert!(doc.page_text(0).unwrap().contains("Hybride"));
    assert!(exercise(doc).contains("Hybride"));
}

#[test]
fn the_last_revision_wins() {
    let doc = open_or_refuse("revisions.pdf").expect("trois révisions");
    assert!(doc.page_text(0).unwrap().contains("Revision 3"));
    assert!(exercise(doc).contains("Revision 3"));
}

#[test]
fn a_linearized_file_opens_and_saves() {
    let doc = open_or_refuse("linearise.pdf").expect("fichier linéarisé");
    assert_eq!(doc.page_count(), 3);
    exercise(doc);
}

#[test]
fn a_decompression_bomb_neither_exhausts_memory_nor_blocks_the_save() {
    let doc = open_or_refuse("bombe-flate.pdf").expect("la bombe s'ouvre");
    exercise(doc);
}

#[test]
fn a_lying_jpeg_header_is_not_decoded() {
    let doc = open_or_refuse("jpeg-piege.pdf").expect("image piégée");
    assert!(exercise(doc).contains("Image piegee"));
}

/// R3 : 5 000 formulaires imbriqués, caviardés sur un thread à **petite pile** (256 Kio) :
/// une récursion sans limite y déborde et ferme le processus.
#[test]
fn deeply_nested_xobjects_do_not_overflow_the_stack() {
    let bytes = corpus("xobjects-profonds.pdf");
    let outcome = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || {
            let mut doc = Document::from_bytes(bytes, None).unwrap();
            doc.apply(&EditCommand::Redact {
                page: 0,
                rect: Rect {
                    x: 60.0,
                    y: 490.0,
                    w: 120.0,
                    h: 120.0,
                },
            })
            .map(|_| ())
        })
        .unwrap()
        .join()
        .expect("le thread ne doit pas paniquer");
    // Prudence : trop profond pour être analysé, le caviardage refuse plutôt que de laisser
    // une image peut-être visible sous la zone.
    assert!(
        matches!(outcome, Err(CoreError::InvalidOperation(_))),
        "{outcome:?}"
    );
}
