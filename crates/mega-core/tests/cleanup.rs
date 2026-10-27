// SPDX-License-Identifier: GPL-3.0-or-later
//! Passe `lopdf` à l'enregistrement : `/InkList` de nos formes, `/AP` des notes, élagage.

use mega_core::testing::sample_pdf;
use mega_core::{AnnotationKind, Color, Document, Geometry, NewAnnotation, Rect, Style};

fn style() -> Style {
    Style {
        color: Color {
            r: 229,
            g: 57,
            b: 53,
        },
        fill: None,
        opacity: 1.0,
        stroke_width: 2.0,
        font_size: 12.0,
    }
}

fn count_objects(bytes: &[u8]) -> usize {
    let text = String::from_utf8_lossy(bytes);
    text.matches(" 0 obj").count()
}

#[test]
fn save_cleanup_strips_note_ap_and_adds_inklist() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("propre.pdf");
    std::fs::write(&path, sample_pdf(1, "cleanup")).unwrap();
    let mut doc = Document::open(&path, None).unwrap();
    doc.add_annotation(
        0,
        &NewAnnotation {
            kind: AnnotationKind::Note,
            geometry: Geometry::Rect {
                rect: Rect {
                    x: 50.0,
                    y: 700.0,
                    w: 0.0,
                    h: 0.0,
                },
            },
            style: style(),
            contents: "Bonjour".to_string(),
            author: "Moi".to_string(),
        },
    )
    .unwrap();
    doc.add_annotation(
        0,
        &NewAnnotation {
            kind: AnnotationKind::Rect,
            geometry: Geometry::Rect {
                rect: Rect {
                    x: 100.0,
                    y: 500.0,
                    w: 200.0,
                    h: 100.0,
                },
            },
            style: style(),
            contents: String::new(),
            author: String::new(),
        },
    )
    .unwrap();
    for _ in 0..5 {
        doc.render_page(0, 0.5).unwrap();
    }
    let raw = doc.to_bytes().unwrap();
    assert!(String::from_utf8_lossy(&raw).contains("PDFIUM_HasGeneratedAP"));
    let before = doc.render_page(0, 1.0).unwrap();
    doc.save().unwrap();
    let saved = std::fs::read(&path).unwrap();
    let text = String::from_utf8_lossy(&saved);
    assert!(
        !text.contains("PDFIUM_HasGeneratedAP"),
        "clé privée retirée"
    );
    assert!(
        text.contains("/InkList"),
        "/InkList écrit pour le rectangle"
    );
    assert!(
        count_objects(&saved) < count_objects(&raw),
        "objets orphelins élagués"
    );
    let reopened = Document::open(&path, None).unwrap();
    let anns = reopened.annotations(0).unwrap();
    assert_eq!(anns.len(), 2);
    assert_eq!(anns[0].kind, AnnotationKind::Note);
    assert_eq!(anns[0].contents.as_deref(), Some("Bonjour"));
    assert_eq!(anns[1].kind, AnnotationKind::Rect);
    assert_eq!(reopened.render_page(0, 1.0).unwrap().rgba, before.rgba);
}

#[test]
fn cleanup_keeps_encrypted_bytes() {
    let bytes = std::fs::read(format!(
        "{}/../../fixtures/encrypted.pdf",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let out = mega_core::cleanup::before_save(&bytes).unwrap();
    assert_eq!(out, bytes);
}

#[test]
fn cleanup_inklist_follows_ink_strokes() {
    let mut doc = Document::from_bytes(sample_pdf(1, "ink"), None).unwrap();
    doc.add_annotation(
        0,
        &NewAnnotation {
            kind: AnnotationKind::Ink,
            geometry: Geometry::Strokes {
                strokes: vec![vec![[10.0, 10.0], [20.0, 30.0], [40.0, 25.0]]],
            },
            style: style(),
            contents: String::new(),
            author: String::new(),
        },
    )
    .unwrap();
    let raw = doc.to_bytes().unwrap();
    let out = mega_core::cleanup::before_save(&raw).unwrap();
    let text = String::from_utf8_lossy(&out);
    let i = text.find("/InkList").expect("InkList");
    let tail = &text[i..i + 80];
    assert!(
        tail.contains("10") && tail.contains("30") && tail.contains("40"),
        "{tail}"
    );
}

/// La clé privée `/PDFIUM_HasGeneratedAP` est posée aussi sur les marquages de texte : elle
/// disparaît du fichier (leur apparence, elle, est conservée : elle est portable).
#[test]
fn save_cleanup_strips_private_key_from_markups_but_keeps_their_appearance() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("marquage.pdf");
    std::fs::write(&path, sample_pdf(1, "marquage")).unwrap();
    let mut doc = Document::open(&path, None).unwrap();
    doc.add_annotation(
        0,
        &NewAnnotation {
            kind: AnnotationKind::Highlight,
            geometry: Geometry::Quads {
                rects: vec![Rect {
                    x: 70.0,
                    y: 750.0,
                    w: 100.0,
                    h: 24.0,
                }],
            },
            style: style(),
            contents: String::new(),
            author: String::new(),
        },
    )
    .unwrap();
    doc.render_page(0, 0.5).unwrap();
    assert!(String::from_utf8_lossy(&doc.to_bytes().unwrap()).contains("PDFIUM_HasGeneratedAP"));
    let before = doc.render_page(0, 1.0).unwrap();
    doc.save().unwrap();
    let saved = std::fs::read(&path).unwrap();
    assert!(!String::from_utf8_lossy(&saved).contains("PDFIUM_HasGeneratedAP"));
    let reopened = Document::open(&path, None).unwrap();
    assert_eq!(reopened.render_page(0, 1.0).unwrap().rgba, before.rgba);
}

fn fixture_bytes(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// B1 (pdf-expert) : un PDF protégé par un mot de passe **propriétaire** seul s'ouvre sans mot
/// de passe ; `lopdf` le déchiffre alors en silence et `is_encrypted()` ment. L'enregistrement
/// doit conserver le chiffrement (octets PDFium tels quels), et les passes `lopdf` refuser.
#[test]
fn owner_password_document_keeps_its_encryption_on_save() {
    let bytes = fixture_bytes("owner-only.pdf");
    assert!(String::from_utf8_lossy(&bytes).contains("/Encrypt"));
    let out = mega_core::cleanup::before_save(&bytes).unwrap();
    assert!(
        String::from_utf8_lossy(&out).contains("/Encrypt"),
        "la passe de nettoyage ne doit pas déchiffrer"
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("owner.pdf");
    std::fs::write(&path, &bytes).unwrap();
    let mut doc = Document::open(&path, None).unwrap();
    doc.rotate_pages(&[0], 1).unwrap();
    doc.save().unwrap();
    let saved = std::fs::read(&path).unwrap();
    assert!(
        String::from_utf8_lossy(&saved).contains("/Encrypt"),
        "toujours chiffré"
    );
    let reopened = Document::open(&path, None).unwrap();
    assert_eq!(reopened.page_count(), 3);
    // Les passes qui réécrivent par lopdf refusent plutôt que de déchiffrer.
    let err = reopened_watermark_error(&path);
    assert!(err.contains("chiffré"), "{err}");
}

fn reopened_watermark_error(path: &std::path::Path) -> String {
    let mut doc = Document::open(path, None).unwrap();
    let spec = mega_core::overlay::Watermark {
        text: "X".to_string(),
        size: 40.0,
        opacity: 0.5,
        angle_deg: 0.0,
        color: Color { r: 0, g: 0, b: 0 },
        pages: None,
    };
    match doc.add_watermark(&spec) {
        Err(mega_core::CoreError::InvalidOperation(m)) => m,
        other => panic!("{other:?}"),
    }
}

/// A5 (pdf-expert) : deux superpositions successives ne doivent pas emboîter deux enveloppes
/// `q … Q` (limite de profondeur d'Acrobat) : la nôtre est reconnue et réutilisée.
#[test]
fn second_overlay_reuses_the_existing_wrapper() {
    let mut doc = Document::from_bytes(sample_pdf(1, "double"), None).unwrap();
    let spec = mega_core::overlay::Watermark {
        text: "UN".to_string(),
        size: 40.0,
        opacity: 0.5,
        angle_deg: 0.0,
        color: Color { r: 0, g: 0, b: 0 },
        pages: None,
    };
    doc.add_watermark(&spec).unwrap();
    doc.add_watermark(&mega_core::overlay::Watermark {
        text: "DEUX".to_string(),
        ..spec
    })
    .unwrap();
    let lo = lopdf::Document::load_mem(&doc.to_bytes().unwrap()).unwrap();
    let page = lo.get_pages()[&1];
    let streams = lo.get_page_contents(page);
    // [q, contenu d'origine, Q, UN, DEUX] : cinq flux, pas sept.
    assert_eq!(streams.len(), 5, "{streams:?}");
    let text: String = doc.text_boxes(0).unwrap().iter().map(|b| b.ch).collect();
    assert!(text.contains("UN") && text.contains("DEUX"));
}
