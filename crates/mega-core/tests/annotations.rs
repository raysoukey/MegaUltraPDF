// SPDX-License-Identifier: GPL-3.0-or-later
//! Annotations : lecture de fichiers tiers, création, apparences, aller-retour.

use mega_core::testing::{annotated_pdf, annotated_pdf_without_annots, sample_pdf};
use mega_core::{AnnotationKind, Color, CoreError, Document};

fn annotated() -> Document {
    Document::from_bytes(annotated_pdf(), None).unwrap()
}

#[test]
fn reads_third_party_annotations() {
    let doc = annotated();
    let anns = doc.annotations(0).unwrap();
    let kinds: Vec<_> = anns.iter().map(|a| a.kind).collect();
    assert_eq!(
        kinds,
        vec![
            AnnotationKind::Note,
            AnnotationKind::Highlight,
            AnnotationKind::Rect,
            AnnotationKind::Underline
        ]
    );
    let note = &anns[0];
    assert_eq!(note.index, 0);
    assert_eq!(note.author.as_deref(), Some("Alice"));
    assert_eq!(note.contents.as_deref(), Some("Note de test"));
    assert!(
        note.created
            .as_deref()
            .unwrap_or("")
            .starts_with("D:20261026")
    );
    assert!(note.modified.is_some());
    assert!((note.rect.x - 50.0).abs() < 0.5 && (note.rect.w - 20.0).abs() < 0.5);
    let hl = &anns[1];
    assert!((hl.opacity - 0.5).abs() < 0.02, "opacité {}", hl.opacity);
    assert_eq!(hl.quads.len(), 1);
    assert!((hl.quads[0].x - 72.0).abs() < 0.5 && (hl.quads[0].w - 128.0).abs() < 0.5);
    assert_eq!(hl.author.as_deref(), Some("Bob"));
    let square = &anns[2];
    assert_eq!(square.color, Some(Color { r: 255, g: 0, b: 0 }));
    assert!((square.rect.w - 200.0).abs() < 0.5 && (square.rect.h - 100.0).abs() < 0.5);
}

#[test]
fn read_annotation_without_color() {
    let doc = annotated();
    let underline = &doc.annotations(0).unwrap()[3];
    assert_eq!(underline.kind, AnnotationKind::Underline);
    // Sans `/C`, PDFium applique la valeur par défaut de la norme : noir, opaque.
    assert_eq!(underline.color, Some(Color { r: 0, g: 0, b: 0 }));
    assert_eq!(underline.opacity, 1.0);
    assert_eq!(underline.author, None);
}

#[test]
fn annotations_out_of_range_page_is_error() {
    let doc = annotated();
    assert!(matches!(
        doc.annotations(3),
        Err(CoreError::PageOutOfRange { index: 3, count: 1 })
    ));
    let plain = Document::from_bytes(sample_pdf(1, "sans"), None).unwrap();
    assert!(plain.annotations(0).unwrap().is_empty());
}

#[test]
fn render_includes_annotations() {
    let with = annotated().render_page(0, 1.0).unwrap();
    let without = Document::from_bytes(annotated_pdf_without_annots(), None)
        .unwrap()
        .render_page(0, 1.0)
        .unwrap();
    assert_eq!(
        (with.width_px, with.height_px),
        (without.width_px, without.height_px)
    );
    assert_ne!(
        with.rgba, without.rgba,
        "les annotations doivent apparaître dans le rendu"
    );
    // Le carré (AP fourni) couvre [100..300] × [500..600] en PDF, soit y écran = 842-600..842-500.
    let px = |img: &mega_core::RenderedPage, x: u32, y: u32| {
        let i = ((y * img.width_px + x) * 4) as usize;
        (img.rgba[i], img.rgba[i + 1], img.rgba[i + 2])
    };
    let inside = px(&with, 200, 842 - 550);
    assert_ne!(inside, (255, 255, 255), "intérieur du carré rempli en bleu");
}
