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
fn insert_text_rejects_characters_missing_from_fonts() {
    let mut doc = one_page();
    let style = text_style(FontFamily::Helvetica, false, false, 12.0);
    let err = doc
        .insert_text(0, [100.0, 500.0], "Bonjour 🙂", &style)
        .unwrap_err();
    match err {
        CoreError::InvalidOperation(m) => assert!(m.contains("non pris en charge"), "{m}"),
        other => panic!("{other:?}"),
    }
    assert!(doc.insert_text(0, [100.0, 500.0], "你", &style).is_err());
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

// --- Lot 14 (audit E9) : image insérée par lopdf ------------------------------------------

/// Insère le fichier `path` sur la page de `doc`, dans `rect` (espace affiché).
fn insert_file(doc: &mut Document, path: &std::path::Path, rect: Rect) {
    doc.apply(&EditCommand::InsertImage {
        page: 0,
        rect,
        path: path.to_string_lossy().into_owned(),
    })
    .unwrap();
}

/// Les objets image (`/Subtype /Image`, masques compris) d'un PDF sérialisé.
fn image_objects(bytes: &[u8]) -> (lopdf::Document, Vec<lopdf::ObjectId>) {
    let doc = lopdf::Document::load_mem(bytes).unwrap();
    let ids = doc
        .objects
        .iter()
        .filter(|(_, o)| {
            o.as_stream()
                .ok()
                .and_then(|s| s.dict.get(b"Subtype").ok())
                .and_then(|t| t.as_name().ok())
                == Some(b"Image".as_slice())
        })
        .map(|(id, _)| *id)
        .collect();
    (doc, ids)
}

fn name_entry(stream: &lopdf::Stream, key: &[u8]) -> Option<Vec<u8>> {
    stream
        .dict
        .get(key)
        .ok()
        .and_then(|v| v.as_name().ok())
        .map(<[u8]>::to_vec)
}

/// Un JPEG « photo » (dégradés et motif, ~15 Ko comme dans l'audit E8).
fn photo_jpeg(gray: bool) -> Vec<u8> {
    let mut img = image::RgbImage::new(200, 150);
    for (x, y, p) in img.enumerate_pixels_mut() {
        *p = image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x * y) % 256) as u8]);
    }
    let mut out = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85);
    if gray {
        encoder
            .encode_image(&image::DynamicImage::ImageRgb8(img).to_luma8())
            .unwrap();
    } else {
        encoder.encode_image(&img).unwrap();
    }
    out
}

/// E9 : un JPEG entre dans le fichier **tel quel** (`/DCTDecode`, octets d'origine), sans
/// masque : PDFium le réencodait en Flate (15 Ko → 85 Ko) avec un `/SMask` opaque inutile.
#[test]
fn inserted_jpeg_keeps_dct_encoding_and_has_no_smask() {
    let dir = tempfile::tempdir().unwrap();
    for (gray, space) in [(false, &b"DeviceRGB"[..]), (true, &b"DeviceGray"[..])] {
        let jpeg = photo_jpeg(gray);
        let path = dir.path().join(format!("photo-{gray}.jpg"));
        std::fs::write(&path, &jpeg).unwrap();
        let mut doc = one_page();
        let before = doc.to_bytes().unwrap();
        insert_file(
            &mut doc,
            &path,
            Rect {
                x: 100.0,
                y: 100.0,
                w: 200.0,
                h: 150.0,
            },
        );
        let out = doc.to_bytes().unwrap();
        let growth = out.len().saturating_sub(before.len());
        assert!(
            growth < jpeg.len() * 6 / 5 + 3 * 1024,
            "gris {gray} : +{growth} octets pour un JPEG de {}",
            jpeg.len()
        );
        let (lo, ids) = image_objects(&out);
        assert_eq!(ids.len(), 1, "gris {gray} : une seule image, aucun masque");
        let image = lo.get_object(ids[0]).unwrap().as_stream().unwrap();
        assert_eq!(
            name_entry(image, b"Filter").as_deref(),
            Some(&b"DCTDecode"[..])
        );
        assert_eq!(name_entry(image, b"ColorSpace").as_deref(), Some(space));
        assert_eq!(image.content, jpeg, "gris {gray} : octets d'origine");
        assert!(!image.dict.has(b"SMask"), "gris {gray}");
        // Posée et ajustée comme avant : 200 × 150 dans 200 × 150.
        let placed = doc.last_image_bounds(0).unwrap().unwrap();
        assert!(
            close(placed.x, 100.0, 0.5)
                && close(placed.y, 100.0, 0.5)
                && close(placed.w, 200.0, 0.5)
                && close(placed.h, 150.0, 0.5),
            "{placed:?}"
        );
        doc.undo().unwrap();
        assert_eq!(image_objects(&doc.to_bytes().unwrap()).1.len(), 0);
    }
}

/// Une image transparente garde sa transparence : un vrai masque `/SMask` (DeviceGray).
#[test]
fn inserted_png_with_alpha_gets_a_real_smask() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("logo.png");
    let mut rgba = image::RgbaImage::new(200, 150);
    for (x, _y, p) in rgba.enumerate_pixels_mut() {
        *p = image::Rgba([20, 120, 200, if x < 100 { 255 } else { 0 }]);
    }
    rgba.save(&path).unwrap();
    let mut doc = one_page();
    insert_file(
        &mut doc,
        &path,
        Rect {
            x: 100.0,
            y: 100.0,
            w: 200.0,
            h: 150.0,
        },
    );
    let (lo, ids) = image_objects(&doc.to_bytes().unwrap());
    assert_eq!(ids.len(), 2, "l'image et son masque");
    let main = ids
        .iter()
        .map(|id| lo.get_object(*id).unwrap().as_stream().unwrap())
        .find(|s| s.dict.has(b"SMask"))
        .expect("une image porte un /SMask");
    assert_eq!(
        name_entry(main, b"ColorSpace").as_deref(),
        Some(&b"DeviceRGB"[..])
    );
    let mask_id = main.dict.get(b"SMask").unwrap().as_reference().unwrap();
    let mask = lo.get_object(mask_id).unwrap().as_stream().unwrap();
    assert_eq!(
        name_entry(mask, b"ColorSpace").as_deref(),
        Some(&b"DeviceGray"[..])
    );
    let alpha = mask.decompressed_content().unwrap();
    assert_eq!(alpha.len(), 200 * 150);
    assert_eq!(alpha[0], 255, "moitié gauche opaque");
    assert_eq!(alpha[199], 0, "moitié droite transparente");
}

/// Un PNG RVBA entièrement opaque n'a pas de masque : il ne servirait à rien.
#[test]
fn inserted_opaque_png_has_no_smask() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("opaque.png");
    image::RgbaImage::from_pixel(80, 40, image::Rgba([200, 30, 30, 255]))
        .save(&path)
        .unwrap();
    let mut doc = one_page();
    insert_file(
        &mut doc,
        &path,
        Rect {
            x: 100.0,
            y: 100.0,
            w: 80.0,
            h: 40.0,
        },
    );
    let (lo, ids) = image_objects(&doc.to_bytes().unwrap());
    assert_eq!(ids.len(), 1, "aucun masque");
    let image = lo.get_object(ids[0]).unwrap().as_stream().unwrap();
    assert!(!image.dict.has(b"SMask"));
    assert_eq!(
        name_entry(image, b"Filter").as_deref(),
        Some(&b"FlateDecode"[..])
    );
    assert_eq!(image.decompressed_content().unwrap().len(), 80 * 40 * 3);
}

/// Pixels sombres du rendu `img` dans le rectangle `(x, y, w, h)` (points, espace affiché,
/// rendu à l'échelle 1).
fn dark_pixels(img: &mega_core::RenderedPage, x: f32, y: f32, w: f32, h: f32) -> usize {
    let mut n = 0;
    for py in (img.height_px as f32 - y - h) as u32..(img.height_px as f32 - y) as u32 {
        for px in x as u32..(x + w) as u32 {
            let i = ((py * img.width_px + px) * 4) as usize;
            if img.rgba[i] < 128 {
                n += 1;
            }
        }
    }
    n
}

/// ADR-0010 : sur une page `/Rotate 90` (affichée 600 × 400), l'image tombe dans le rectangle
/// **affiché** et reste droite à l'écran (moitié noire à gauche, moitié blanche à droite).
#[test]
fn inserted_image_on_a_rotated_page_lands_at_the_displayed_rectangle() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("moitie.png");
    let mut img = image::RgbImage::from_pixel(300, 100, image::Rgb([255, 255, 255]));
    for (x, _y, p) in img.enumerate_pixels_mut() {
        if x < 150 {
            *p = image::Rgb([0, 0, 0]);
        }
    }
    img.save(&path).unwrap();
    let mut doc = Document::from_bytes(rotated_pdf(), None).unwrap();
    insert_file(
        &mut doc,
        &path,
        Rect {
            x: 50.0,
            y: 50.0,
            w: 240.0,
            h: 80.0,
        },
    );
    let render = doc.render_page(0, 1.0).unwrap();
    assert_eq!((render.width_px, render.height_px), (600, 400));
    let inner = 110 * 70;
    let left = dark_pixels(&render, 55.0, 55.0, 110.0, 70.0);
    assert!(
        left * 100 >= inner * 95,
        "moitié gauche noire : {left}/{inner}"
    );
    assert_eq!(
        dark_pixels(&render, 175.0, 55.0, 110.0, 70.0),
        0,
        "moitié droite blanche"
    );
    assert_eq!(
        dark_pixels(&render, 300.0, 150.0, 180.0, 80.0),
        0,
        "rien ailleurs"
    );
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

// --- Lot 10 : police Unicode incorporée -------------------------------------------------

#[test]
fn insert_greek_text_is_extractable() {
    let mut doc = one_page();
    let style = text_style(FontFamily::Helvetica, true, false, 14.0);
    doc.insert_text(0, [100.0, 500.0], "Ελλάδα Łódź\nПривет", &style)
        .unwrap();
    let text = doc.page_text(0).unwrap();
    assert!(
        text.contains("Ελλάδα Łódź") && text.contains("Привет"),
        "{text:?}"
    );
}

#[test]
fn unicode_family_is_accepted_for_plain_text() {
    let mut doc = one_page();
    let style = text_style(FontFamily::Unicode, false, false, 14.0);
    doc.insert_text(0, [100.0, 500.0], "abc", &style).unwrap();
    assert!(doc.page_text(0).unwrap().contains("abc"));
}

/// Retouche d'un texte en police standard (non incorporée) avec des lettres hors WinAnsi :
/// l'objet passe en police Unicode, au même endroit.
#[test]
fn set_object_text_switches_a_standard_font_to_unicode() {
    let mut doc = one_page();
    let before = doc.page_objects(0).unwrap();
    let index = before
        .iter()
        .position(|o| o.kind == mega_core::ObjectKind::Text)
        .unwrap() as u16;
    doc.set_object_text(0, index, "Страница 1").unwrap();
    let text = doc.page_text(0).unwrap();
    assert!(text.contains("Страница 1"), "{text:?}");
    let after = doc.page_objects(0).unwrap();
    assert_eq!(after.len(), before.len(), "objet remplacé, pas ajouté");
    let (b, a) = (
        before[usize::from(index)].bounds,
        after[usize::from(index)].bounds,
    );
    assert!(
        (a.x - b.x).abs() < 1.0 && (a.y - b.y).abs() < 3.0,
        "{b:?} → {a:?}"
    );
}

/// Deux insertions séparées par un rechargement (filigrane) chargent deux fois la police
/// dans PDFium : l'enregistrement n'en garde qu'un programme.
#[test]
fn two_unicode_insertions_embed_one_font_after_save() {
    let mut doc = one_page();
    let style = text_style(FontFamily::Unicode, false, false, 14.0);
    doc.insert_text(0, [100.0, 500.0], "Ω", &style).unwrap();
    doc.add_watermark(&mega_core::Watermark {
        text: "X".to_string(),
        size: 20.0,
        opacity: 0.5,
        angle_deg: 0.0,
        color: Color { r: 0, g: 0, b: 0 },
        pages: None,
    })
    .unwrap();
    doc.insert_text(0, [100.0, 400.0], "Ж", &style).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unicode.pdf");
    doc.save_as(&path).unwrap();
    let saved = lopdf::Document::load_mem(&std::fs::read(&path).unwrap()).unwrap();
    let programs = saved
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .filter_map(|d| d.get(b"FontFile2").ok())
        .filter_map(|r| r.as_reference().ok())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(programs.len(), 1, "un seul programme de police");
    assert!(doc.page_text(0).unwrap().contains('Ж'));
}

/// Programmes `/FontFile2` d'un fichier enregistré (objets distincts) et leurs octets.
fn font_files(saved: &lopdf::Document) -> Vec<(lopdf::ObjectId, usize)> {
    let ids = saved
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .filter_map(|d| d.get(b"FontFile2").ok())
        .filter_map(|r| r.as_reference().ok())
        .collect::<std::collections::BTreeSet<_>>();
    ids.into_iter()
        .map(|id| {
            (
                id,
                saved
                    .get_object(id)
                    .unwrap()
                    .as_stream()
                    .unwrap()
                    .content
                    .len(),
            )
        })
        .collect()
}

/// Lot 14 (audit E10) : PDFium incorpore la police Unicode **entière** (≈ 386 Ko) ;
/// l'enregistrement n'en garde que les glyphes dessinés, sans changer le rendu ni le texte.
#[test]
fn inserted_unicode_text_embeds_a_subset_font() {
    let simple = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/simple.pdf"
    ))
    .unwrap();
    let mut doc = Document::from_bytes(simple, None).unwrap();
    let style = text_style(FontFamily::Unicode, false, false, 24.0);
    doc.insert_text(0, [100.0, 500.0], "Ω", &style).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("subset.pdf");
    doc.save_as(&path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let saved = lopdf::Document::load_mem(&bytes).unwrap();
    let files = font_files(&saved);
    assert_eq!(files.len(), 1, "{files:?}");
    assert!(files[0].1 < 40 * 1024, "police de {} octets", files[0].1);
    let tagged = saved
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .filter_map(|d| d.get(b"BaseFont").ok()?.as_name().ok())
        .filter(|name| name.ends_with(b"DejaVuSans"))
        .collect::<Vec<_>>();
    assert!(!tagged.is_empty());
    for name in tagged {
        assert!(
            name.len() == 17 && name[..6].iter().all(u8::is_ascii_uppercase) && name[6] == b'+',
            "{}",
            String::from_utf8_lossy(name)
        );
    }
    let reopened = Document::from_bytes(bytes, None).unwrap();
    assert!(reopened.page_text(0).unwrap().contains('Ω'));
    let diff = mega_core::compare::compare_pages(&doc, 0, &reopened, 0, 1.0).unwrap();
    assert_eq!(
        diff.differing_pixels, 0,
        "rendu changé par le sous-ensemble"
    );
}
