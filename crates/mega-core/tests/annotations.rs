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
    // Sans `/C`, et sans apparence dans le fichier : la lecture déclenche un rendu minuscule
    // qui fait générer l'apparence par PDFium avec la couleur par défaut de la norme (noir).
    let underline = &doc.annotations(0).unwrap()[3];
    assert_eq!(underline.kind, AnnotationKind::Underline);
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
    // La couleur d'une note n'est jamais lue : `/C` est inaccessible sans risque et le
    // symbole généré par PDFium est jaune quelle que soit la couleur demandée.
    assert_eq!(a.color, None);
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

// ---- Passe de correction après relecture (Lot 3) ----

/// Pixel RGB d'un rendu à (x, y) en coordonnées image (origine en haut à gauche).
fn pixel(img: &mega_core::RenderedPage, x: u32, y: u32) -> (u8, u8, u8) {
    let i = ((y * img.width_px + x) * 4) as usize;
    (img.rgba[i], img.rgba[i + 1], img.rgba[i + 2])
}

/// B1 (pdf-expert) : `/QuadPoints` dans l'ordre attendu par Acrobat et PDFium
/// (haut-gauche, haut-droit, bas-gauche, bas-droit), sinon PDFium génère une apparence de
/// largeur nulle et l'enregistre : le marquage est invisible partout.
#[test]
fn text_markups_are_visible_after_render_and_reopen() {
    for kind in [
        AnnotationKind::Highlight,
        AnnotationKind::Underline,
        AnnotationKind::StrikeOut,
    ] {
        let mut doc = Document::from_bytes(sample_pdf(1, "quad"), None).unwrap();
        // Zone sans texte : 100..300 × 500..520 (PDF) → ligne image y = 842 - 510.
        doc.add_annotation(
            0,
            &new(
                kind,
                Geometry::Quads {
                    rects: vec![rect(100.0, 500.0, 200.0, 20.0)],
                },
                style((255, 0, 0)),
                "",
            ),
        )
        .unwrap();
        let img = doc.render_page(0, 1.0).unwrap();
        let probe = |img: &mega_core::RenderedPage| {
            // Surlignage : plein ; souligné : ligne en bas ; barré : ligne au milieu.
            let ys: [u32; 3] = [842 - 510, 842 - 501, 842 - 511];
            ys.iter().any(|&y| pixel(img, 200, y) != (255, 255, 255))
        };
        assert!(probe(&img), "{kind:?} invisible après rendu");
        let reopened = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
        let again = reopened.render_page(0, 1.0).unwrap();
        assert!(probe(&again), "{kind:?} invisible après réouverture");
        let q = &reopened.annotations(0).unwrap()[0].quads[0];
        assert!(close(q.x, 100.0, 0.5) && close(q.w, 200.0, 0.5) && close(q.y, 500.0, 0.5));
    }
}

/// M1 (pdf-expert) : une note ne doit ni zoomer ni tourner avec la page (§12.5.6.4).
#[test]
fn notes_are_flagged_no_zoom_no_rotate() {
    let mut doc = Document::from_bytes(sample_pdf(1, "flags"), None).unwrap();
    doc.add_annotation(
        0,
        &new(
            AnnotationKind::Note,
            Geometry::Rect {
                rect: rect(100.0, 700.0, 0.0, 0.0),
            },
            style((255, 200, 0)),
            "n",
        ),
    )
    .unwrap();
    let bytes = doc.to_bytes().unwrap();
    let text = String::from_utf8_lossy(&bytes);
    // Print (4) + NoZoom (8) + NoRotate (16) = 28.
    assert!(
        text.contains("/F 28"),
        "drapeaux de la note : {}",
        &text[..0]
    );
}

// ---- Passe de correction après relecture (Fable) ----

/// C2 : une encre tierce lissée en courbes de Bézier reste une encre, sans géométrie
/// reconstruite, et ne peut pas être « déplacée » (ce qui la redessinerait en ellipse).
#[test]
fn third_party_bezier_ink_stays_ink_and_is_not_rescaled() {
    let doc = Document::from_bytes(mega_core::testing::bezier_ink_pdf(), None).unwrap();
    let a = &doc.annotations(0).unwrap()[0];
    assert_eq!(a.kind, AnnotationKind::Ink);
    assert!(a.geometry.is_none(), "{:?}", a.geometry);
    assert!(!a.editable);
    let mut doc = doc;
    assert!(matches!(
        doc.set_annotation_rect(0, 0, rect(0.0, 0.0, 50.0, 50.0)),
        Err(CoreError::InvalidOperation(_))
    ));
    assert_eq!(
        doc.annotations(0).unwrap().len(),
        1,
        "rien n'a été supprimé"
    );
}

/// I1 : déplacer trois fois une forme ne change pas sa taille.
#[test]
fn moving_a_shape_three_times_keeps_its_size() {
    let mut doc = Document::from_bytes(sample_pdf(1, "stable"), None).unwrap();
    let mut s = style((229, 57, 53));
    s.stroke_width = 2.0;
    doc.add_annotation(
        0,
        &new(
            AnnotationKind::Rect,
            Geometry::Rect {
                rect: rect(100.0, 500.0, 200.0, 100.0),
            },
            s,
            "",
        ),
    )
    .unwrap();
    let mut index = 0;
    let first = doc.annotations(0).unwrap()[0].rect;
    for i in 1..=3 {
        let r = rect(first.x + 5.0 * i as f32, first.y, first.w, first.h);
        index = doc.set_annotation_rect(0, index, r).unwrap();
    }
    let last = doc.annotations(0).unwrap()[usize::from(index)].rect;
    assert!(
        close(last.w, first.w, 0.1) && close(last.h, first.h, 0.1),
        "{first:?} → {last:?}"
    );
    assert!(close(last.x, first.x + 15.0, 0.1));
    // Idem pour une ellipse éditée (modifier = recréer).
    let mut doc = Document::from_bytes(sample_pdf(1, "ell"), None).unwrap();
    doc.add_annotation(
        0,
        &new(
            AnnotationKind::Ellipse,
            Geometry::Rect {
                rect: rect(100.0, 300.0, 100.0, 50.0),
            },
            s,
            "",
        ),
    )
    .unwrap();
    let first = doc.annotations(0).unwrap()[0].rect;
    let mut index = 0;
    for _ in 0..3 {
        index = doc
            .update_annotation(
                0,
                index,
                &mega_core::AnnotationPatch {
                    color: Some(Color { r: 0, g: 0, b: 255 }),
                    ..Default::default()
                },
            )
            .unwrap();
    }
    let last = doc.annotations(0).unwrap()[usize::from(index)].rect;
    assert!(
        close(last.w, first.w, 0.1) && close(last.h, first.h, 0.1),
        "{first:?} → {last:?}"
    );
}

/// I3 : la couleur d'un marquage est connue dès sa création (l'apparence manquante est
/// générée par un rendu minuscule avant la lecture).
#[test]
fn markup_color_is_known_right_after_creation() {
    let mut doc = Document::from_bytes(sample_pdf(1, "fresh"), None).unwrap();
    let mut s = style((255, 212, 0));
    s.opacity = 0.5;
    doc.add_annotation(
        0,
        &new(
            AnnotationKind::Highlight,
            Geometry::Quads {
                rects: vec![rect(72.0, 700.0, 128.0, 20.0)],
            },
            s,
            "",
        ),
    )
    .unwrap();
    let a = &doc.annotations(0).unwrap()[0];
    assert_eq!(
        a.color,
        Some(Color {
            r: 255,
            g: 212,
            b: 0
        })
    );
    assert!(close(a.opacity, 0.5, 0.02));
    assert!(a.editable);
}

/// I2 : une note se modifie **sur place** (contenu, auteur) : même index, `/C` conservé,
/// et sa couleur n'est pas déduite du symbole jaune de PDFium.
#[test]
fn note_update_is_in_place_and_keeps_its_color() {
    let mut doc = Document::from_bytes(sample_pdf(1, "note"), None).unwrap();
    doc.add_annotation(
        0,
        &new(
            AnnotationKind::Note,
            Geometry::Rect {
                rect: rect(100.0, 700.0, 0.0, 0.0),
            },
            style((0, 128, 255)),
            "v1",
        ),
    )
    .unwrap();
    doc.add_annotation(
        0,
        &new(
            AnnotationKind::Rect,
            Geometry::Rect {
                rect: rect(10.0, 10.0, 50.0, 50.0),
            },
            style((0, 0, 0)),
            "",
        ),
    )
    .unwrap();
    doc.render_page(0, 1.0).unwrap();
    let idx = doc
        .update_annotation(
            0,
            0,
            &mega_core::AnnotationPatch {
                contents: Some("v2".to_string()),
                author: Some("Moi".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(idx, 0, "modifiée sur place");
    let anns = doc.annotations(0).unwrap();
    assert_eq!(anns.len(), 2);
    assert_eq!(anns[0].contents.as_deref(), Some("v2"));
    assert_eq!(anns[0].author.as_deref(), Some("Moi"));
    assert_eq!(
        anns[0].color, None,
        "le jaune du symbole PDFium n'est pas une couleur lue"
    );
    let bytes = doc.to_bytes().unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains("/C[ 0 .50196"),
        "le /C bleu d'origine est conservé"
    );
}

/// I5 : enregistrer repart d'un document propre (les objets orphelins créés par les rendus
/// ne sont pas écrits).
#[test]
fn save_does_not_carry_render_garbage() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("propre.pdf");
    std::fs::write(&path, sample_pdf(1, "garbage")).unwrap();
    let mut doc = Document::open(&path, None).unwrap();
    for i in 0..5 {
        doc.add_annotation(
            0,
            &new(
                AnnotationKind::Note,
                Geometry::Rect {
                    rect: rect(50.0 + 30.0 * i as f32, 700.0, 0.0, 0.0),
                },
                style((255, 200, 0)),
                "Bonjour",
            ),
        )
        .unwrap();
    }
    for _ in 0..10 {
        doc.render_page(0, 0.5).unwrap();
    }
    let dirty = doc.to_bytes().unwrap().len();
    doc.save().unwrap();
    let saved = std::fs::metadata(&path).unwrap().len() as usize;
    assert!(
        saved * 2 < dirty,
        "enregistré {saved} o, en mémoire {dirty} o"
    );
    let reopened = Document::open(&path, None).unwrap();
    assert_eq!(reopened.annotations(0).unwrap().len(), 5);
}

/// I6 : `editable` dit à l'interface ce qu'elle peut déplacer ou modifier ; une note tierce
/// déplacée garde sa taille.
#[test]
fn editable_flag_and_third_party_note_size() {
    let doc = annotated();
    let anns = doc.annotations(0).unwrap();
    assert!(anns[0].editable, "note tierce");
    assert!(anns[1].editable, "surlignage tierce");
    assert!(!anns[2].editable, "carré tiers avec /AP : non recréable");
    let mut doc = doc;
    // La note tierce fait 20×20 ; on la déplace : même taille, même index.
    let idx = doc
        .set_annotation_rect(0, 0, rect(200.0, 300.0, 20.0, 20.0))
        .unwrap();
    assert_eq!(idx, 0);
    let note = &doc.annotations(0).unwrap()[0];
    assert!(close(note.rect.x, 200.0, 0.5) && close(note.rect.w, 20.0, 0.5));
}

/// M1 : une modification invalide ne supprime pas l'annotation (validation avant suppression).
#[test]
fn invalid_update_keeps_the_annotation() {
    let mut doc = Document::from_bytes(sample_pdf(1, "inv"), None).unwrap();
    doc.add_annotation(
        0,
        &new(
            AnnotationKind::Rect,
            Geometry::Rect {
                rect: rect(10.0, 10.0, 50.0, 50.0),
            },
            style((0, 0, 0)),
            "",
        ),
    )
    .unwrap();
    assert!(matches!(
        doc.update_annotation(
            0,
            0,
            &mega_core::AnnotationPatch {
                opacity: Some(0.0),
                ..Default::default()
            }
        ),
        Err(CoreError::InvalidOperation(_))
    ));
    assert_eq!(doc.annotations(0).unwrap().len(), 1);
}

// --- Lot 9 : dates, validation (B1, B5, B9) ----------------------------------------------

/// Modifier une annotation la recrée : sa date de création d'origine doit survivre.
#[test]
fn modifying_keeps_the_creation_date() {
    let mut doc = Document::from_bytes(sample_pdf(1, "dates"), None).unwrap();
    doc.add_annotation(
        0,
        &new(
            AnnotationKind::Rect,
            Geometry::Rect {
                rect: rect(100.0, 100.0, 100.0, 50.0),
            },
            style((229, 57, 53)),
            "",
        ),
    )
    .unwrap();
    // Une date ancienne, comme si l'annotation avait été créée en 2020 : réécrite en place,
    // à la même largeur (l'annotation fraîche est écrite directement dans `/Annots`).
    let mut bytes = doc.to_bytes().unwrap();
    let key = b"/CreationDate";
    let at = bytes.windows(key.len()).position(|w| w == key).unwrap();
    let start = at + bytes[at..].windows(2).position(|w| w == b"D:").unwrap() + 2;
    bytes[start..start + 14].copy_from_slice(b"20200101120000");
    let mut doc = Document::from_bytes(bytes, None).unwrap();
    let index = doc.annotations(0).unwrap()[0].index;
    doc.update_annotation(
        0,
        index,
        &mega_core::AnnotationPatch {
            color: Some(Color { r: 0, g: 0, b: 255 }),
            ..Default::default()
        },
    )
    .unwrap();
    let a = &doc.annotations(0).unwrap()[0];
    assert!(
        a.created
            .as_deref()
            .is_some_and(|d| d.starts_with("D:2020")),
        "{:?}",
        a.created
    );
    assert!(a.modified.is_some(), "/M posé");
}

#[test]
fn non_win_ansi_text_is_refused_where_helvetica_draws_it() {
    let mut doc = Document::from_bytes(sample_pdf(1, "ansi"), None).unwrap();
    for kind in [AnnotationKind::Text, AnnotationKind::Stamp] {
        let refused = doc.add_annotation(
            0,
            &new(
                kind,
                Geometry::Rect {
                    rect: rect(72.0, 600.0, 200.0, 40.0),
                },
                style((0, 0, 0)),
                "Ωmega",
            ),
        );
        assert!(
            matches!(refused, Err(CoreError::InvalidOperation(_))),
            "{kind:?} : {refused:?}"
        );
    }
    // Une note garde son commentaire Unicode (il n'est pas dessiné en Helvetica).
    doc.add_annotation(
        0,
        &new(
            AnnotationKind::Note,
            Geometry::Rect {
                rect: rect(72.0, 600.0, 20.0, 20.0),
            },
            style((255, 204, 0)),
            "Ωmega",
        ),
    )
    .unwrap();
}

#[test]
fn nan_and_infinite_values_are_refused() {
    let mut doc = Document::from_bytes(sample_pdf(1, "nan"), None).unwrap();
    let mut thick = style((0, 0, 0));
    thick.stroke_width = f32::NAN;
    let mut huge_font = style((0, 0, 0));
    huge_font.font_size = f32::INFINITY;
    let cases = [
        new(
            AnnotationKind::Rect,
            Geometry::Rect {
                rect: rect(10.0, 10.0, 50.0, 50.0),
            },
            thick,
            "",
        ),
        new(
            AnnotationKind::Text,
            Geometry::Rect {
                rect: rect(10.0, 10.0, 50.0, 50.0),
            },
            huge_font,
            "texte",
        ),
        new(
            AnnotationKind::Rect,
            Geometry::Rect {
                rect: rect(f32::NAN, 10.0, 50.0, 50.0),
            },
            style((0, 0, 0)),
            "",
        ),
        new(
            AnnotationKind::Line,
            Geometry::Line {
                from: [0.0, 0.0],
                to: [f32::INFINITY, 5.0],
            },
            style((0, 0, 0)),
            "",
        ),
    ];
    for case in cases {
        assert!(
            matches!(
                doc.add_annotation(0, &case),
                Err(CoreError::InvalidOperation(_))
            ),
            "{case:?}"
        );
    }
}
