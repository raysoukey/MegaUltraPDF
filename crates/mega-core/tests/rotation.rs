// SPDX-License-Identifier: GPL-3.0-or-later
//! Pages tournées (`/Rotate`) : toute l'API publique parle en coordonnées **affichées**
//! (la page telle que PDFium la rend), et c'est `mega-core` qui convertit vers l'espace
//! utilisateur du fichier.

use mega_core::testing::rotated_pdf;
use mega_core::{
    AnnotationKind, Color, Document, Geometry, NewAnnotation, Rect, SignatureSource, Style,
};

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, w, h }
}

fn close(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

fn same_rect(a: &Rect, b: &Rect, tol: f32) -> bool {
    close(a.x, b.x, tol) && close(a.y, b.y, tol) && close(a.w, b.w, tol) && close(a.h, b.h, tol)
}

fn pixel(img: &mega_core::RenderedPage, x: u32, y: u32) -> (u8, u8, u8) {
    let i = ((y * img.width_px + x) * 4) as usize;
    (img.rgba[i], img.rgba[i + 1], img.rgba[i + 2])
}

fn red() -> Style {
    Style {
        color: Color {
            r: 229,
            g: 57,
            b: 53,
        },
        fill: Some(Color {
            r: 229,
            g: 57,
            b: 53,
        }),
        opacity: 1.0,
        stroke_width: 2.0,
        font_size: 14.0,
    }
}

#[test]
fn rotated_fixture_is_displayed_landscape() {
    let doc = Document::from_bytes(rotated_pdf(), None).unwrap();
    let size = doc.page_size(0).unwrap();
    // MediaBox 400 × 600 avec /Rotate 90 : affichée 600 × 400.
    assert_eq!((size.width_pt, size.height_pt), (600.0, 400.0));
    let frame = doc.page_frame(0).unwrap();
    assert_eq!(frame.rotate, 90);
    assert!(same_rect(&frame.bbox, &rect(0.0, 0.0, 400.0, 600.0), 0.01));
}

#[test]
fn annotations_on_rotated_page_round_trip() {
    let mut doc = Document::from_bytes(rotated_pdf(), None).unwrap();
    let displayed = rect(50.0, 30.0, 100.0, 40.0);
    doc.add_annotation(
        0,
        &NewAnnotation {
            kind: AnnotationKind::Rect,
            geometry: Geometry::Rect { rect: displayed },
            style: red(),
            contents: String::new(),
            author: String::new(),
        },
    )
    .unwrap();
    // Relu identique, en coordonnées affichées.
    let reopened = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    let anns = reopened.annotations(0).unwrap();
    let shape = anns
        .iter()
        .find(|a| a.kind == AnnotationKind::Rect)
        .expect("le rectangle");
    assert_eq!(
        shape.geometry,
        Some(Geometry::Rect { rect: displayed }),
        "géométrie relue en espace affiché"
    );
    assert!(shape.editable);
    // Dans le fichier, le /Rect est en espace utilisateur : x ≈ 330..370, y ≈ 50..150.
    let lo = lopdf::Document::load_mem(&reopened.to_bytes().unwrap()).unwrap();
    let page_id = lo.get_pages()[&1];
    let page = lo.get_dictionary(page_id).unwrap();
    let annots = lo.dereference(page.get(b"Annots").unwrap()).unwrap().1;
    let ink = annots
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|o| lo.dereference(o).ok())
        .filter_map(|(_, o)| o.as_dict().ok().cloned())
        .find(|d| d.get(b"Subtype").and_then(|s| s.as_name()).ok() == Some(b"Ink".as_slice()))
        .expect("annotation Ink dans le fichier");
    let user: Vec<f32> = ink
        .get(b"Rect")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o.as_float().unwrap())
        .collect();
    assert!(
        user[0] > 320.0 && user[2] < 380.0 && user[1] > 40.0 && user[3] < 160.0,
        "/Rect utilisateur : {user:?}"
    );
    // Rendu : le centre du rectangle affiché (100, 50) est rouge.
    let img = reopened.render_page(0, 1.0).unwrap();
    assert_eq!((img.width_px, img.height_px), (600, 400));
    let (r, g, b) = pixel(&img, 100, 400 - 50);
    assert!(r > 200 && g < 100 && b < 100, "pixel {:?}", (r, g, b));
    // Et là où un /Rect non converti serait tombé (utilisateur 50..150 × 30..70, soit
    // affiché 30..70 × 250..350), c'est blanc.
    let (r, g, b) = pixel(&img, 50, 400 - 300);
    assert_eq!((r, g, b), (255, 255, 255));
}

#[test]
fn text_annotation_on_rotated_page_keeps_font_size_and_moves() {
    let mut doc = Document::from_bytes(rotated_pdf(), None).unwrap();
    let displayed = rect(300.0, 200.0, 200.0, 40.0);
    let index = doc
        .add_annotation(
            0,
            &NewAnnotation {
                kind: AnnotationKind::Text,
                geometry: Geometry::Rect { rect: displayed },
                style: red(),
                contents: "Bonjour".to_string(),
                author: String::new(),
            },
        )
        .unwrap();
    let ann = doc
        .annotations(0)
        .unwrap()
        .into_iter()
        .find(|a| a.index == index)
        .unwrap();
    assert_eq!(ann.kind, AnnotationKind::Text);
    assert!(same_rect(&ann.rect, &displayed, 0.5), "{:?}", ann.rect);
    assert!(
        close(ann.font_size.unwrap(), 14.0, 0.1),
        "{:?}",
        ann.font_size
    );
    assert!(ann.editable);
    // Le texte est dessiné **dans** son rectangle affiché : un pixel rouge y existe.
    let img = doc.render_page(0, 2.0).unwrap();
    let mut found = false;
    for y in 0..80u32 {
        for x in 0..400u32 {
            let (r, g, b) = pixel(&img, 600 + x, (400 - 240) * 2 + y);
            if r > 150 && g < 120 && b < 120 {
                found = true;
            }
        }
    }
    assert!(found, "texte rouge attendu dans le rectangle affiché");
    // Déplacement en coordonnées affichées.
    let moved = rect(100.0, 100.0, 200.0, 40.0);
    let index = doc.set_annotation_rect(0, index, moved).unwrap();
    let ann = doc
        .annotations(0)
        .unwrap()
        .into_iter()
        .find(|a| a.index == index)
        .unwrap();
    assert!(same_rect(&ann.rect, &moved, 0.5), "{:?}", ann.rect);
}

#[test]
fn form_fields_on_rotated_page() {
    let doc = Document::from_bytes(rotated_pdf(), None).unwrap();
    let fields = doc.form_fields(0).unwrap();
    assert_eq!(fields.len(), 1);
    // Widget /Rect [50 500 250 530] en espace utilisateur → affiché x 500..530, y 150..350.
    assert!(
        same_rect(&fields[0].rect, &rect(500.0, 150.0, 30.0, 200.0), 0.5),
        "{:?}",
        fields[0].rect
    );
}

#[test]
fn signature_on_rotated_page() {
    let mut doc = Document::from_bytes(rotated_pdf(), None).unwrap();
    let zone = rect(300.0, 100.0, 200.0, 60.0);
    doc.insert_signature(
        0,
        zone,
        &SignatureSource::Strokes {
            strokes: vec![vec![[310.0, 110.0], [490.0, 150.0]]],
        },
    )
    .unwrap();
    let img = doc.render_page(0, 1.0).unwrap();
    // Milieu du trait, en pixels affichés : (400, 400 - 130).
    let (r, g, b) = pixel(&img, 400, 400 - 130);
    assert!(r < 128 && g < 128 && b < 128, "pixel {:?}", (r, g, b));
    // Image : emprise relue en espace affiché, ajustée dans la zone.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sig.png");
    image::RgbaImage::from_pixel(200, 100, image::Rgba([0, 0, 255, 255]))
        .save(&path)
        .unwrap();
    doc.insert_signature(
        0,
        zone,
        &SignatureSource::Image {
            path: path.to_string_lossy().into_owned(),
        },
    )
    .unwrap();
    let placed = doc.last_image_bounds(0).unwrap().unwrap();
    // 200 × 100 dans 200 × 60 → 120 × 60, centré : x 340..460, y 100..160.
    assert!(
        same_rect(&placed, &rect(340.0, 100.0, 120.0, 60.0), 0.5),
        "{placed:?}"
    );
    let img = doc.render_page(0, 1.0).unwrap();
    let (r, g, b) = pixel(&img, 400, 400 - 130);
    assert!(b > 200 && r < 60 && g < 60, "pixel {:?}", (r, g, b));
}

#[test]
fn text_boxes_on_rotated_page_are_in_displayed_space() {
    let doc = Document::from_bytes(rotated_pdf(), None).unwrap();
    let boxes = doc.text_boxes(0).unwrap();
    let p = boxes
        .iter()
        .find(|b| b.ch == 'P')
        .expect("le P de « Page 1 »");
    // « Page 1 » en Helvetica 24 à (72, 560) : affiché, la ligne de base devient x ≈ 560 et
    // le P se trouve vers y ≈ 400 − 72 − 17.
    assert!(p.rect.x > 540.0 && p.rect.x < 585.0, "{:?}", p.rect);
    assert!(p.rect.y > 300.0 && p.rect.y < 330.0, "{:?}", p.rect);
    let hits = doc
        .search_page(0, "Page", &mega_core::SearchOptions::default())
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].rects[0].x > 540.0, "{:?}", hits[0].rects);
}
