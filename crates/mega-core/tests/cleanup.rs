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
