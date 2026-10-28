// SPDX-License-Identifier: GPL-3.0-or-later
//! Objets de page : lecture, suppression, déplacement, remplacement du texte.

use mega_core::edit::EditCommand;
use mega_core::testing::{fragile_content_pdf, rotated_pdf, sample_pdf};
use mega_core::{
    Color, CoreError, Document, FontFamily, ObjectKind, Rect, SignatureSource, TextStyle,
};

fn one_page() -> Document {
    Document::from_bytes(sample_pdf(1, "objets"), None).unwrap()
}

fn page_text(doc: &Document, page: u16) -> String {
    doc.text_boxes(page)
        .unwrap()
        .iter()
        .map(|b| b.ch)
        .filter(|c| !c.is_whitespace())
        .collect()
}

fn close(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn reads_page_objects() {
    let doc = one_page();
    let objects = doc.page_objects(0).unwrap();
    assert_eq!(objects.len(), 1);
    let o = &objects[0];
    assert_eq!(o.index, 0);
    assert_eq!(o.kind, ObjectKind::Text);
    assert_eq!(o.text.as_deref(), Some("Page 1"));
    assert!(close(o.font_size.unwrap(), 24.0, 0.5), "{:?}", o.font_size);
    // « Page 1 » en Helvetica 24 posé à (72, 760) : emprise plausible.
    assert!(close(o.bounds.x, 72.0, 3.0), "{:?}", o.bounds);
    assert!(o.bounds.y > 740.0 && o.bounds.y < 762.0, "{:?}", o.bounds);
    assert!(o.bounds.w > 50.0 && o.bounds.w < 120.0, "{:?}", o.bounds);
    assert!(o.bounds.h > 10.0 && o.bounds.h < 30.0, "{:?}", o.bounds);
}

#[test]
fn delete_object_changes_rendering_and_undo() {
    let mut doc = one_page();
    let before = doc.render_page(0, 0.5).unwrap();
    doc.apply(&EditCommand::DeleteObject { page: 0, index: 0 })
        .unwrap();
    assert_eq!(doc.undo_label().as_deref(), Some("Supprimer : Texte"));
    assert!(doc.page_objects(0).unwrap().is_empty());
    assert_eq!(page_text(&doc, 0), "");
    let after = doc.render_page(0, 0.5).unwrap();
    assert_ne!(before.rgba, after.rgba);
    // La suppression survit à la sérialisation.
    let reopened = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    assert!(reopened.page_objects(0).unwrap().is_empty());
    doc.undo().unwrap();
    assert_eq!(doc.page_objects(0).unwrap().len(), 1);
    assert_eq!(doc.render_page(0, 0.5).unwrap().rgba, before.rgba);
}

#[test]
fn move_object_shifts_bounds() {
    let mut doc = one_page();
    let before = doc.page_objects(0).unwrap()[0].bounds;
    doc.apply(&EditCommand::MoveObject {
        page: 0,
        index: 0,
        dx: 10.0,
        dy: -20.0,
    })
    .unwrap();
    assert_eq!(doc.undo_label().as_deref(), Some("Déplacer : Texte"));
    let after = doc.page_objects(0).unwrap()[0].bounds;
    assert!(close(after.x, before.x + 10.0, 0.1), "{after:?}");
    assert!(close(after.y, before.y - 20.0, 0.1), "{after:?}");
    assert!(close(after.w, before.w, 0.1) && close(after.h, before.h, 0.1));
    let reopened = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    let kept = reopened.page_objects(0).unwrap()[0].bounds;
    assert!(close(kept.x, after.x, 0.1) && close(kept.y, after.y, 0.1));
}

#[test]
fn move_object_on_rotated_page_follows_displayed_axes() {
    let mut doc = Document::from_bytes(rotated_pdf(), None).unwrap();
    let before = doc.page_objects(0).unwrap()[0].bounds;
    doc.move_object(0, 0, 10.0, 0.0).unwrap();
    let after = doc.page_objects(0).unwrap()[0].bounds;
    assert!(
        close(after.x, before.x + 10.0, 0.1),
        "{before:?} → {after:?}"
    );
    assert!(close(after.y, before.y, 0.1), "{before:?} → {after:?}");
}

#[test]
fn set_object_text_round_trips() {
    let mut doc = one_page();
    doc.apply(&EditCommand::SetObjectText {
        page: 0,
        index: 0,
        text: "Bonjour".to_string(),
    })
    .unwrap();
    assert_eq!(doc.undo_label().as_deref(), Some("Modifier le texte"));
    assert_eq!(page_text(&doc, 0), "Bonjour");
    assert_eq!(
        doc.page_objects(0).unwrap()[0].text.as_deref(),
        Some("Bonjour")
    );
    let reopened = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    assert_eq!(page_text(&reopened, 0), "Bonjour");
    doc.undo().unwrap();
    assert_eq!(page_text(&doc, 0), "Page1");
}

#[test]
fn unknown_object_index_is_refused() {
    let mut doc = one_page();
    for result in [
        doc.delete_object(0, 5),
        doc.move_object(0, 5, 1.0, 1.0),
        doc.set_object_text(0, 5, "x"),
    ] {
        assert!(
            matches!(result, Err(CoreError::InvalidOperation(_))),
            "{result:?}"
        );
    }
    assert!(matches!(
        doc.page_objects(3),
        Err(CoreError::PageOutOfRange { .. })
    ));
    assert_eq!(doc.page_objects(0).unwrap().len(), 1, "rien n'a changé");
}

#[test]
fn set_object_text_validates_text_and_kind() {
    let mut doc = one_page();
    assert!(matches!(
        doc.set_object_text(0, 0, "   "),
        Err(CoreError::InvalidOperation(_))
    ));
    let long = "a".repeat(4097);
    assert!(matches!(
        doc.set_object_text(0, 0, &long),
        Err(CoreError::InvalidOperation(_))
    ));
    // Un objet chemin (trait de signature) n'a pas de texte.
    doc.insert_signature(
        0,
        Rect {
            x: 100.0,
            y: 100.0,
            w: 200.0,
            h: 60.0,
        },
        &SignatureSource::Strokes {
            strokes: vec![vec![[100.0, 110.0], [200.0, 150.0]]],
        },
    )
    .unwrap();
    let objects = doc.page_objects(0).unwrap();
    assert_eq!(objects.len(), 2);
    assert_eq!(objects[1].kind, ObjectKind::Path);
    assert!(objects[1].text.is_none());
    assert!(matches!(
        doc.set_object_text(0, 1, "x"),
        Err(CoreError::InvalidOperation(_))
    ));
    assert_eq!(page_text(&doc, 0), "Page1");
}

fn text_style(family: FontFamily, bold: bool, italic: bool, size: f32) -> TextStyle {
    TextStyle {
        family,
        bold,
        italic,
        size,
        color: Color { r: 0, g: 0, b: 0 },
    }
}

fn insert_text(page: u16, at: [f32; 2], text: &str, style: TextStyle) -> EditCommand {
    EditCommand::InsertText {
        page,
        at,
        text: text.to_string(),
        style,
    }
}

#[test]
fn insert_text_is_extractable_and_undoable() {
    let mut doc = one_page();
    let before = doc.render_page(0, 0.5).unwrap();
    doc.apply(&insert_text(
        0,
        [100.0, 500.0],
        "Bonjour\nmonde",
        text_style(FontFamily::Helvetica, false, false, 18.0),
    ))
    .unwrap();
    assert_eq!(doc.undo_label().as_deref(), Some("Insérer du texte"));
    let text = page_text(&doc, 0);
    assert!(text.contains("Bonjour") && text.contains("monde"), "{text}");
    let objects = doc.page_objects(0).unwrap();
    assert_eq!(objects.len(), 3, "une ligne = un objet texte");
    let (first, second) = (&objects[1], &objects[2]);
    assert_eq!(first.kind, ObjectKind::Text);
    assert_eq!(first.text.as_deref(), Some("Bonjour"));
    assert!(close(first.font_size.unwrap(), 18.0, 0.1));
    assert_eq!(first.font.as_deref(), Some("Helvetica"));
    // Le point donné est le coin haut-gauche du bloc : la première ligne commence là et
    // tient sous 500 ; la seconde est plus bas d'environ 1,2 × 18.
    assert!(close(first.bounds.x, 100.0, 2.0), "{:?}", first.bounds);
    let top = first.bounds.y + first.bounds.h;
    assert!(top <= 500.5 && top > 480.0, "{:?}", first.bounds);
    // (les bas des emprises dépendent des jambages : on compare les hauts, « B » et « d »
    // montant tous deux jusqu'à la hauteur d'ascendante.)
    let second_top = second.bounds.y + second.bounds.h;
    assert!(
        close(second_top, top - 21.6, 2.0),
        "{:?} / {:?}",
        first.bounds,
        second.bounds
    );
    assert_ne!(doc.render_page(0, 0.5).unwrap().rgba, before.rgba);
    let reopened = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    assert!(page_text(&reopened, 0).contains("Bonjour"));
    doc.undo().unwrap();
    assert_eq!(page_text(&doc, 0), "Page1");
    assert_eq!(doc.render_page(0, 0.5).unwrap().rgba, before.rgba);
}

#[test]
fn insert_text_rejects_non_winansi() {
    let mut doc = one_page();
    let style = text_style(FontFamily::Helvetica, false, false, 12.0);
    let err = doc
        .insert_text(0, [100.0, 500.0], "Bonjour 🙂", &style)
        .unwrap_err();
    match err {
        CoreError::InvalidOperation(m) => assert!(m.contains("non pris en charge"), "{m}"),
        other => panic!("{other:?}"),
    }
    assert!(doc.insert_text(0, [100.0, 500.0], "Σ", &style).is_err());
    // L'alphabet latin étendu de WinAnsi passe (é, —, «, », €, œ).
    doc.insert_text(0, [100.0, 500.0], "élève — « ok » 5 € œuf", &style)
        .unwrap();
    assert!(page_text(&doc, 0).contains("élève"));
    // Autres refus : texte vide, taille absurde, point hors page.
    assert!(doc.insert_text(0, [100.0, 500.0], "  \n ", &style).is_err());
    let tiny = text_style(FontFamily::Helvetica, false, false, 2.0);
    assert!(doc.insert_text(0, [100.0, 500.0], "x", &tiny).is_err());
    assert!(doc.insert_text(0, [700.0, 500.0], "x", &style).is_err());
    assert_eq!(
        doc.page_objects(0).unwrap().len(),
        2,
        "rien d'autre n'a été ajouté"
    );
}

#[test]
fn insert_text_style_bold_italic() {
    let mut doc = one_page();
    let cases = [
        (FontFamily::Helvetica, true, true, "Helvetica-BoldOblique"),
        (FontFamily::Times, true, false, "Times-Bold"),
        (FontFamily::Courier, false, true, "Courier-Oblique"),
        (FontFamily::Times, false, false, "Times-Roman"),
    ];
    for (i, (family, bold, italic, _)) in cases.iter().enumerate() {
        doc.insert_text(
            0,
            [72.0, 600.0 - 30.0 * i as f32],
            "Style",
            &text_style(*family, *bold, *italic, 14.0),
        )
        .unwrap();
    }
    let reopened = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    let objects = reopened.page_objects(0).unwrap();
    let fonts: Vec<&str> = objects[1..]
        .iter()
        .map(|o| o.font.as_deref().unwrap_or(""))
        .collect();
    let expected: Vec<&str> = cases.iter().map(|c| c.3).collect();
    assert_eq!(fonts, expected);
}

#[test]
fn insert_text_on_rotated_page_is_upright() {
    let mut doc = Document::from_bytes(rotated_pdf(), None).unwrap();
    doc.insert_text(
        0,
        [300.0, 300.0],
        "Bonjour",
        &text_style(FontFamily::Helvetica, false, false, 24.0),
    )
    .unwrap();
    let objects = doc.page_objects(0).unwrap();
    let o = objects.last().unwrap();
    assert_eq!(o.text.as_deref(), Some("Bonjour"));
    assert!(close(o.font_size.unwrap(), 24.0, 0.1), "{:?}", o.font_size);
    // Horizontal à l'écran : plus large que haut, posé au point demandé (espace affiché).
    assert!(o.bounds.w > o.bounds.h, "{:?}", o.bounds);
    assert!(close(o.bounds.x, 300.0, 2.0), "{:?}", o.bounds);
    assert!(o.bounds.y + o.bounds.h <= 300.5, "{:?}", o.bounds);
}

#[test]
fn insert_image_fits_rect() {
    let mut doc = one_page();
    let before = doc.render_page(0, 0.5).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("logo.png");
    image::RgbaImage::from_pixel(200, 100, image::Rgba([0, 0, 255, 255]))
        .save(&path)
        .unwrap();
    doc.apply(&EditCommand::InsertImage {
        page: 0,
        rect: Rect {
            x: 100.0,
            y: 100.0,
            w: 100.0,
            h: 100.0,
        },
        path: path.to_string_lossy().into_owned(),
    })
    .unwrap();
    assert_eq!(doc.undo_label().as_deref(), Some("Insérer une image"));
    // 200 × 100 dans 100 × 100 → 100 × 50, centré verticalement.
    let placed = doc.last_image_bounds(0).unwrap().unwrap();
    assert!(
        close(placed.x, 100.0, 0.5)
            && close(placed.y, 125.0, 0.5)
            && close(placed.w, 100.0, 0.5)
            && close(placed.h, 50.0, 0.5),
        "{placed:?}"
    );
    let objects = doc.page_objects(0).unwrap();
    assert_eq!(objects.len(), 2);
    assert_eq!(objects[1].kind, ObjectKind::Image);
    assert!(close(objects[1].bounds.y, 125.0, 0.5));
    assert_ne!(doc.render_page(0, 0.5).unwrap().rgba, before.rgba);
    doc.undo().unwrap();
    assert_eq!(doc.page_objects(0).unwrap().len(), 1);
    // Refus : fichier absent, zone vide, image trop grande en pixels.
    let missing = dir.path().join("absent.png").to_string_lossy().into_owned();
    assert!(matches!(
        doc.insert_image(0, placed, &missing),
        Err(CoreError::Io(_))
    ));
    let empty = Rect {
        x: 100.0,
        y: 100.0,
        w: 0.0,
        h: 10.0,
    };
    let path = path.to_string_lossy().into_owned();
    assert!(doc.insert_image(0, empty, &path).is_err());
    let huge = dir.path().join("huge.png");
    image::GrayImage::new(8193, 1).save(&huge).unwrap();
    assert!(matches!(
        doc.insert_image(0, placed, &huge.to_string_lossy()),
        Err(CoreError::InvalidOperation(_))
    ));
}

/// B3 (pdf-expert) : PDFium régénère tout le flux d'une page modifiée et y **perd** le texte
/// en police Type 3, les images en ligne et les ombrages. Modifier un objet existant sur une
/// telle page est refusé ; y **ajouter** (nouveau flux) reste permis.
#[test]
fn editing_objects_is_refused_when_regeneration_would_lose_content() {
    for kind in ["type3", "inline", "shading"] {
        let mut doc = Document::from_bytes(fragile_content_pdf(kind), None).unwrap();
        let before = doc.render_page(0, 0.5).unwrap();
        for result in [
            doc.delete_object(0, 0),
            doc.move_object(0, 0, 5.0, 0.0),
            doc.set_object_text(0, 0, "x"),
            doc.redact(
                0,
                Rect {
                    x: 60.0,
                    y: 690.0,
                    w: 100.0,
                    h: 40.0,
                },
            )
            .map(|_| ()),
        ] {
            match result {
                Err(CoreError::InvalidOperation(m)) => {
                    assert!(m.contains("régénér"), "{kind} : {m}")
                }
                other => panic!("{kind} : {other:?}"),
            }
        }
        assert_eq!(
            doc.render_page(0, 0.5).unwrap().rgba,
            before.rgba,
            "{kind} intact"
        );
        // L'insertion ajoute un flux : rien d'existant n'est régénéré.
        doc.insert_text(
            0,
            [72.0, 400.0],
            "ok",
            &text_style(FontFamily::Helvetica, false, false, 12.0),
        )
        .unwrap();
        assert!(page_text(&doc, 0).contains("ok"), "{kind}");
    }
}

/// A8 (pdf-expert) : PDFium écrit les pixels non compressés ; un budget en pixels borne la
/// taille du fichier (et de chaque instantané d'historique).
#[test]
fn insert_image_refuses_more_than_sixteen_megapixels() {
    let mut doc = one_page();
    let dir = tempfile::tempdir().unwrap();
    let big = dir.path().join("big.png");
    image::GrayImage::new(8000, 2100).save(&big).unwrap();
    match doc.insert_image(
        0,
        Rect {
            x: 50.0,
            y: 50.0,
            w: 400.0,
            h: 400.0,
        },
        &big.to_string_lossy(),
    ) {
        Err(CoreError::InvalidOperation(m)) => assert!(m.contains("pixels"), "{m}"),
        other => panic!("{other:?}"),
    }
}

// --- Lot 9 : police sous-ensemble (B22), redimensionnement (B31) -------------------------

#[test]
fn editing_text_with_a_new_glyph_in_a_subset_font_is_refused() {
    let mut doc = Document::from_bytes(mega_core::testing::subset_font_pdf(), None).unwrap();
    let refused = doc.apply(&EditCommand::SetObjectText {
        page: 0,
        index: 0,
        text: "Sexy".to_string(),
    });
    assert!(
        matches!(refused, Err(CoreError::InvalidOperation(ref m)) if m.contains("x")),
        "{refused:?}"
    );
    // Les caractères déjà présents restent permis.
    doc.apply(&EditCommand::SetObjectText {
        page: 0,
        index: 0,
        text: "terce".to_string(),
    })
    .unwrap();
}

/// Une image insérée dans 100 × 100, puis redimensionnée dans `target`.
fn resized_image(rotate_first: bool, target: Rect) -> Rect {
    let mut doc = one_page();
    if rotate_first {
        doc.apply(&EditCommand::RotatePages {
            pages: vec![0],
            quarter_turns: 1,
        })
        .unwrap();
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("carre.png");
    image::RgbaImage::from_pixel(100, 100, image::Rgba([0, 128, 0, 255]))
        .save(&path)
        .unwrap();
    doc.apply(&EditCommand::InsertImage {
        page: 0,
        rect: Rect {
            x: 100.0,
            y: 100.0,
            w: 100.0,
            h: 100.0,
        },
        path: path.to_string_lossy().into_owned(),
    })
    .unwrap();
    let index = doc.page_objects(0).unwrap().len() as u16 - 1;
    doc.apply(&EditCommand::ResizeObject {
        page: 0,
        index,
        rect: target,
    })
    .unwrap();
    assert_eq!(doc.undo_label().as_deref(), Some("Redimensionner : Image"));
    let bounds = doc.page_objects(0).unwrap()[usize::from(index)].bounds;
    doc.undo().unwrap();
    let back = doc.page_objects(0).unwrap()[usize::from(index)].bounds;
    assert!(close(back.w, 100.0, 0.5), "annulé : {back:?}");
    bounds
}

#[test]
fn resize_object_scales_an_image_and_is_undoable() {
    let target = Rect {
        x: 150.0,
        y: 200.0,
        w: 200.0,
        h: 50.0,
    };
    for rotate_first in [false, true] {
        let got = resized_image(rotate_first, target);
        assert!(
            close(got.x, target.x, 0.5)
                && close(got.y, target.y, 0.5)
                && close(got.w, target.w, 0.5)
                && close(got.h, target.h, 0.5),
            "page tournée = {rotate_first} : {got:?}"
        );
    }
}
