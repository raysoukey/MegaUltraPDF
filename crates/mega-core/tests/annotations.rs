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
    // Un rendu d'abord : PDFium génère alors l'apparence des marquages sans `/AP`, et c'est
    // par cette apparence que leur couleur et leur opacité deviennent lisibles (voir
    // `color_of`). Dans l'application, une page est toujours rendue avant d'être annotée.
    doc.render_page(0, 1.0).unwrap();
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
    // Avant tout rendu : pas d'apparence, et `/C` n'est lu directement que pour les notes
    // (contournement d'un plantage de pdfium-render) → couleur inconnue, jamais d'erreur.
    let underline = &doc.annotations(0).unwrap()[3];
    assert_eq!(underline.kind, AnnotationKind::Underline);
    assert_eq!(underline.color, None);
    assert_eq!(underline.opacity, 1.0);
    // Après un rendu, PDFium a généré l'apparence avec la couleur par défaut de la norme.
    doc.render_page(0, 1.0).unwrap();
    let underline = &doc.annotations(0).unwrap()[3];
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

// ---- Création, apparences, aller-retour (Task 2) ----

use mega_core::{Geometry, NewAnnotation, Style};

fn style(color: (u8, u8, u8)) -> Style {
    Style {
        color: Color {
            r: color.0,
            g: color.1,
            b: color.2,
        },
        fill: None,
        opacity: 1.0,
        stroke_width: 2.0,
        font_size: 14.0,
    }
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> mega_core::Rect {
    mega_core::Rect { x, y, w, h }
}

fn new(kind: AnnotationKind, geometry: Geometry, style: Style, contents: &str) -> NewAnnotation {
    NewAnnotation {
        kind,
        geometry,
        style,
        contents: contents.to_string(),
        author: "Moi".to_string(),
    }
}

/// Crée, sérialise, rouvre : ce que verra un autre lecteur.
fn round_trip(new: &NewAnnotation) -> mega_core::Annotation {
    let mut doc = Document::from_bytes(sample_pdf(1, "rt"), None).unwrap();
    let index = doc.add_annotation(0, new).unwrap();
    assert_eq!(index, 0);
    let reopened = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    let anns = reopened.annotations(0).unwrap();
    assert_eq!(anns.len(), 1);
    anns.into_iter().next().unwrap()
}

fn close(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn highlight_round_trip() {
    let mut s = style((255, 255, 0));
    s.opacity = 0.5;
    let a = round_trip(&new(
        AnnotationKind::Highlight,
        Geometry::Quads {
            rects: vec![rect(72.0, 700.0, 128.0, 20.0)],
        },
        s,
        "",
    ));
    assert_eq!(a.kind, AnnotationKind::Highlight);
    assert_eq!(a.quads.len(), 1);
    assert!(close(a.quads[0].x, 72.0, 0.5) && close(a.quads[0].w, 128.0, 0.5));
    assert_eq!(a.author.as_deref(), Some("Moi"));
    assert!(a.created.is_some());
    // Un marquage n'a pas d'apparence avant son premier rendu : PDFium la génère alors.
    // Dans les deux cas la couleur, si elle est lue, est le jaune demandé.
    assert!(
        a.color.is_none()
            || a.color
                == Some(Color {
                    r: 255,
                    g: 255,
                    b: 0
                })
    );
    assert!(a.to_new().is_some());
}

#[test]
fn note_round_trip() {
    let a = round_trip(&new(
        AnnotationKind::Note,
        Geometry::Rect {
            rect: rect(100.0, 700.0, 0.0, 0.0),
        },
        style((255, 200, 0)),
        "Bonjour",
    ));
    assert_eq!(a.kind, AnnotationKind::Note);
    assert_eq!(a.contents.as_deref(), Some("Bonjour"));
    assert!(close(a.rect.w, 20.0, 0.5) && close(a.rect.h, 20.0, 0.5));
    assert!(close(a.rect.x, 100.0, 0.5));
    // Pas de lecture directe de `/C` (voir `color_of`) : inconnue avant rendu…
    assert_eq!(a.color, None);
    // …puis lue dans le symbole de note que PDFium génère au premier rendu.
    let doc = Document::from_bytes(
        {
            let mut d = Document::from_bytes(sample_pdf(1, "n"), None).unwrap();
            d.add_annotation(
                0,
                &new(
                    AnnotationKind::Note,
                    Geometry::Rect {
                        rect: rect(100.0, 700.0, 0.0, 0.0),
                    },
                    style((255, 200, 0)),
                    "Bonjour",
                ),
            )
            .unwrap();
            d.to_bytes().unwrap()
        },
        None,
    )
    .unwrap();
    doc.render_page(0, 1.0).unwrap();
    let rendered = &doc.annotations(0).unwrap()[0];
    assert!(
        rendered.color.is_some(),
        "couleur lue dans l'apparence générée"
    );
    assert_ne!(
        rendered.color,
        Some(Color { r: 0, g: 0, b: 0 }),
        "pas le noir du contour"
    );
}

#[test]
fn shapes_round_trip() {
    let mut s = style((229, 57, 53));
    s.fill = Some(Color { r: 0, g: 0, b: 255 });
    let r = round_trip(&new(
        AnnotationKind::Rect,
        Geometry::Rect {
            rect: rect(100.0, 500.0, 200.0, 100.0),
        },
        s,
        "",
    ));
    assert_eq!(r.kind, AnnotationKind::Rect);
    assert!(
        close(r.rect.x, 98.0, 1.5) && close(r.rect.w, 204.0, 3.0),
        "{:?}",
        r.rect
    );
    assert_eq!(
        r.color,
        Some(Color {
            r: 229,
            g: 57,
            b: 53
        })
    );
    assert_eq!(r.fill, Some(Color { r: 0, g: 0, b: 255 }));
    assert!(close(r.stroke_width, 2.0, 0.1));
    assert!(matches!(r.geometry, Some(Geometry::Rect { .. })));

    let e = round_trip(&new(
        AnnotationKind::Ellipse,
        Geometry::Rect {
            rect: rect(100.0, 300.0, 100.0, 50.0),
        },
        style((0, 128, 0)),
        "",
    ));
    assert_eq!(e.kind, AnnotationKind::Ellipse);

    let l = round_trip(&new(
        AnnotationKind::Line,
        Geometry::Line {
            from: [50.0, 50.0],
            to: [250.0, 150.0],
        },
        style((0, 0, 0)),
        "",
    ));
    assert_eq!(l.kind, AnnotationKind::Line);
    assert!(matches!(
        l.geometry,
        Some(Geometry::Line { from, to }) if close(from[0], 50.0, 0.5) && close(to[1], 150.0, 0.5)
    ));

    let f = round_trip(&new(
        AnnotationKind::Arrow,
        Geometry::Line {
            from: [50.0, 400.0],
            to: [250.0, 400.0],
        },
        style((0, 0, 0)),
        "",
    ));
    assert_eq!(f.kind, AnnotationKind::Arrow);
}

#[test]
fn ink_round_trip() {
    let a = round_trip(&new(
        AnnotationKind::Ink,
        Geometry::Strokes {
            strokes: vec![
                vec![[10.0, 10.0], [20.0, 30.0], [40.0, 25.0]],
                vec![[60.0, 60.0], [80.0, 90.0]],
            ],
        },
        style((0, 0, 255)),
        "",
    ));
    assert_eq!(a.kind, AnnotationKind::Ink);
    // Englobant des points, plus épaisseur/2 + 1 de chaque côté.
    assert!(
        close(a.rect.x, 8.0, 0.5) && close(a.rect.w, 74.0, 1.0),
        "{:?}",
        a.rect
    );
    match a.geometry {
        Some(Geometry::Strokes { strokes }) => {
            assert_eq!(strokes.len(), 2);
            assert_eq!(strokes[0].len(), 3);
        }
        other => panic!("géométrie inattendue : {other:?}"),
    }
}

#[test]
fn text_annotation_round_trips_unicode_contents() {
    let a = round_trip(&new(
        AnnotationKind::Text,
        Geometry::Rect {
            rect: rect(72.0, 600.0, 200.0, 40.0),
        },
        style((30, 136, 229)),
        "été — ok\nligne 2",
    ));
    assert_eq!(a.kind, AnnotationKind::Text);
    assert_eq!(a.contents.as_deref(), Some("été — ok\nligne 2"));
    assert!(close(a.font_size.unwrap_or(0.0), 14.0, 0.5));
    assert_eq!(
        a.color,
        Some(Color {
            r: 30,
            g: 136,
            b: 229
        })
    );
}

#[test]
fn stamp_round_trip() {
    let a = round_trip(&new(
        AnnotationKind::Stamp,
        Geometry::Rect {
            rect: rect(300.0, 700.0, 150.0, 40.0),
        },
        style((200, 0, 0)),
        "BROUILLON",
    ));
    assert_eq!(a.kind, AnnotationKind::Stamp);
    assert_eq!(a.contents.as_deref(), Some("BROUILLON"));
}

#[test]
fn annotated_render_differs_from_plain() {
    let mut doc = Document::from_bytes(sample_pdf(1, "rendu"), None).unwrap();
    let before = doc.render_page(0, 1.0).unwrap();
    doc.add_annotation(
        0,
        &new(
            AnnotationKind::Rect,
            Geometry::Rect {
                rect: rect(100.0, 500.0, 200.0, 100.0),
            },
            style((229, 57, 53)),
            "",
        ),
    )
    .unwrap();
    let after = doc.render_page(0, 1.0).unwrap();
    assert_ne!(before.rgba, after.rgba);
    // Et après réouverture (ce que voit un autre lecteur).
    let reopened = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    assert_eq!(reopened.render_page(0, 1.0).unwrap().rgba, after.rgba);
}

#[test]
fn delete_and_move_annotation() {
    let mut doc = Document::from_bytes(sample_pdf(1, "dm"), None).unwrap();
    doc.add_annotation(
        0,
        &new(
            AnnotationKind::Note,
            Geometry::Rect {
                rect: rect(50.0, 700.0, 0.0, 0.0),
            },
            style((255, 200, 0)),
            "n",
        ),
    )
    .unwrap();
    doc.add_annotation(
        0,
        &new(
            AnnotationKind::Rect,
            Geometry::Rect {
                rect: rect(100.0, 500.0, 200.0, 100.0),
            },
            style((229, 57, 53)),
            "",
        ),
    )
    .unwrap();
    // Déplacer la note : même index, nouveau coin.
    assert_eq!(
        doc.set_annotation_rect(0, 0, rect(200.0, 300.0, 20.0, 20.0))
            .unwrap(),
        0
    );
    let anns = doc.annotations(0).unwrap();
    assert!(close(anns[0].rect.x, 200.0, 0.5) && close(anns[0].rect.y, 300.0, 0.5));
    // Déplacer/redimensionner la forme : recréée en fin de tableau, géométrie à l'échelle.
    let idx = doc
        .set_annotation_rect(0, 1, rect(0.0, 0.0, 100.0, 50.0))
        .unwrap();
    let anns = doc.annotations(0).unwrap();
    assert_eq!(anns.len(), 2);
    let moved = &anns[usize::from(idx)];
    assert_eq!(moved.kind, AnnotationKind::Rect);
    assert!(
        close(moved.rect.x, 0.0, 1.5) && close(moved.rect.w, 100.0, 3.0),
        "{:?}",
        moved.rect
    );
    // Supprimer.
    doc.delete_annotation(0, 0).unwrap();
    assert_eq!(doc.annotations(0).unwrap().len(), 1);
}

#[test]
fn delete_unknown_annotation_is_refused() {
    let mut doc = Document::from_bytes(sample_pdf(1, "du"), None).unwrap();
    assert!(matches!(
        doc.delete_annotation(0, 3),
        Err(CoreError::InvalidOperation(_))
    ));
    assert!(matches!(
        doc.delete_annotation(5, 0),
        Err(CoreError::PageOutOfRange { .. })
    ));
}

#[test]
fn style_validation() {
    let mut doc = Document::from_bytes(sample_pdf(1, "sv"), None).unwrap();
    let mut bad = new(
        AnnotationKind::Rect,
        Geometry::Rect {
            rect: rect(0.0, 0.0, 10.0, 10.0),
        },
        style((0, 0, 0)),
        "",
    );
    bad.style.opacity = 0.0;
    assert!(matches!(
        doc.add_annotation(0, &bad),
        Err(CoreError::InvalidOperation(_))
    ));
    bad.style.opacity = 1.0;
    bad.style.stroke_width = 0.0;
    assert!(matches!(
        doc.add_annotation(0, &bad),
        Err(CoreError::InvalidOperation(_))
    ));
    let empty = new(
        AnnotationKind::Highlight,
        Geometry::Quads { rects: vec![] },
        style((0, 0, 0)),
        "",
    );
    assert!(matches!(
        doc.add_annotation(0, &empty),
        Err(CoreError::InvalidOperation(_))
    ));
    let link = new(
        AnnotationKind::Link,
        Geometry::Rect {
            rect: rect(0.0, 0.0, 10.0, 10.0),
        },
        style((0, 0, 0)),
        "",
    );
    assert!(matches!(
        doc.add_annotation(0, &link),
        Err(CoreError::InvalidOperation(_))
    ));
    assert!(doc.annotations(0).unwrap().is_empty(), "rien n'a été créé");
}
