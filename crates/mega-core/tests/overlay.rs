// SPDX-License-Identifier: GPL-3.0-or-later
//! Filigrane, numérotation et recadrage : des flux de contenu ajoutés par `lopdf` (donc
//! exacts dans tous les lecteurs) et un `/CropBox` posé par PDFium.

use mega_core::edit::EditCommand;
use mega_core::overlay::{Corner, CropSpec, PageNumbering, Watermark};
use mega_core::testing::{many_pages_pdf, rotated_pdf, sample_pdf};
use mega_core::{Color, CoreError, Document};

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

fn watermark(text: &str, pages: Option<Vec<u16>>) -> Watermark {
    Watermark {
        text: text.to_string(),
        size: 60.0,
        opacity: 0.3,
        angle_deg: 45.0,
        color: Color {
            r: 128,
            g: 128,
            b: 128,
        },
        pages,
    }
}

fn numbering(template: &str, corner: Corner, start: u16) -> PageNumbering {
    PageNumbering {
        template: template.to_string(),
        corner,
        margin: 20.0,
        size: 10.0,
        start,
        pages: None,
    }
}

#[test]
fn watermark_is_extractable_on_every_page() {
    let mut doc = Document::from_bytes(sample_pdf(3, "filigrane"), None).unwrap();
    let before = doc.render_page(1, 0.5).unwrap();
    doc.apply(&EditCommand::Watermark {
        spec: watermark("BROUILLON", None),
    })
    .unwrap();
    assert_eq!(doc.undo_label().as_deref(), Some("Filigraner 3 pages"));
    for p in 0..3 {
        let text = page_text(&doc, p);
        assert!(text.contains("BROUILLON"), "page {p} : {text}");
        assert!(
            text.contains(&format!("Page{}", p + 1)),
            "page {p} : {text}"
        );
    }
    let after = doc.render_page(1, 0.5).unwrap();
    assert_ne!(before.rgba, after.rgba);
    let reopened = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    assert!(page_text(&reopened, 2).contains("BROUILLON"));
    assert_eq!(reopened.render_page(1, 0.5).unwrap().rgba, after.rgba);
    doc.undo().unwrap();
    assert_eq!(page_text(&doc, 1), "Page2");
    assert_eq!(doc.render_page(1, 0.5).unwrap().rgba, before.rgba);
    // Une plage : seule la page 2 est marquée.
    doc.apply(&EditCommand::Watermark {
        spec: watermark("CONFIDENTIEL", Some(vec![1])),
    })
    .unwrap();
    assert_eq!(doc.undo_label().as_deref(), Some("Filigraner 1 page"));
    assert!(!page_text(&doc, 0).contains("CONFIDENTIEL"));
    assert!(page_text(&doc, 1).contains("CONFIDENTIEL"));
    assert!(!page_text(&doc, 2).contains("CONFIDENTIEL"));
}

#[test]
fn watermark_on_bare_page() {
    // Page écrite à la main : ni `/Resources`, ni `/Contents`.
    let mut doc = Document::from_bytes(many_pages_pdf(1), None).unwrap();
    let before = doc.render_page(0, 1.0).unwrap();
    doc.add_watermark(&watermark("BROUILLON", None)).unwrap();
    assert_eq!(page_text(&doc, 0), "BROUILLON");
    let after = doc.render_page(0, 1.0).unwrap();
    assert_ne!(before.rgba, after.rgba);
    let reopened = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    assert_eq!(page_text(&reopened, 0), "BROUILLON");
}

#[test]
fn watermark_is_validated() {
    let mut doc = Document::from_bytes(sample_pdf(1, "v"), None).unwrap();
    let mut w = watermark("", None);
    assert!(doc.add_watermark(&w).is_err(), "texte vide");
    w.text = "你好".to_string();
    let err = doc.add_watermark(&w).unwrap_err().to_string();
    assert!(
        err.contains("你") && err.contains("好"),
        "caractères absents listés : {err}"
    );
    w = watermark("ok", None);
    w.opacity = 0.0;
    assert!(doc.add_watermark(&w).is_err(), "opacité nulle");
    w = watermark("ok", Some(vec![4]));
    assert!(matches!(
        doc.add_watermark(&w),
        Err(CoreError::PageOutOfRange { .. })
    ));
    w = watermark("ok", Some(vec![]));
    assert!(doc.add_watermark(&w).is_err(), "aucune page");
    assert_eq!(page_text(&doc, 0), "Page1", "rien n'a été écrit");
}

#[test]
fn page_numbers_follow_template() {
    let mut doc = Document::from_bytes(sample_pdf(3, "Rapport"), None).unwrap();
    doc.apply(&EditCommand::PageNumbers {
        spec: numbering("{n} / {total} — {titre}", Corner::BottomCenter, 1),
    })
    .unwrap();
    assert_eq!(doc.undo_label().as_deref(), Some("Numéroter 3 pages"));
    assert_eq!(page_text(&doc, 0), "Page11/3—Rapport");
    assert_eq!(page_text(&doc, 2), "Page33/3—Rapport");
    // En pied, centré : le « 1 » est près du bas et autour du milieu de la page.
    let boxes = doc.text_boxes(0).unwrap();
    let digit = boxes
        .iter()
        .find(|b| b.ch == '1' && b.rect.y < 100.0)
        .expect("le numéro en pied");
    assert!(close(digit.rect.y, 20.0, 4.0), "{:?}", digit.rect);
    assert!(
        digit.rect.x > 220.0 && digit.rect.x < 300.0,
        "{:?}",
        digit.rect
    );
    let reopened = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    assert_eq!(page_text(&reopened, 1), "Page22/3—Rapport");
    doc.undo().unwrap();
    assert_eq!(page_text(&doc, 0), "Page1");
    // En-tête à droite, numérotation commençant à 5.
    doc.add_page_numbers(&numbering("p. {n}", Corner::TopRight, 5))
        .unwrap();
    // L'en-tête est en haut de la page : en ordre de lecture (correctif 0.12.2), il vient avant
    // le corps, même s'il est dessiné après.
    assert_eq!(page_text(&doc, 0), "p.5Page1");
    let boxes = doc.text_boxes(0).unwrap();
    let p = boxes.iter().find(|b| b.ch == 'p').expect("le p. en tête");
    assert!(p.rect.y > 800.0 && p.rect.x > 540.0, "{:?}", p.rect);
    // « p. 5 » fait ~16 pt de large en corps 10 : son bord droit respecte la marge de 20.
    assert!(
        p.rect.x + 16.0 <= 595.28 - 20.0 + 1.0,
        "marge droite : {:?}",
        p.rect
    );
    // Modèle vide ou sans rendu : refusé.
    assert!(
        doc.add_page_numbers(&numbering("  ", Corner::TopLeft, 1))
            .is_err()
    );
}

#[test]
fn crop_changes_page_size_and_resets() {
    let mut doc = Document::from_bytes(sample_pdf(2, "crop"), None).unwrap();
    let full = doc.page_size(0).unwrap();
    doc.apply(&EditCommand::CropPages {
        spec: CropSpec {
            pages: Some(vec![0]),
            margins: [50.0, 50.0, 50.0, 50.0],
        },
    })
    .unwrap();
    assert_eq!(doc.undo_label().as_deref(), Some("Recadrer 1 page"));
    let cropped = doc.page_size(0).unwrap();
    assert!(
        close(cropped.width_pt, full.width_pt - 100.0, 0.1),
        "{cropped:?}"
    );
    assert!(
        close(cropped.height_pt, full.height_pt - 100.0, 0.1),
        "{cropped:?}"
    );
    assert_eq!(doc.page_size(1).unwrap(), full, "page 2 intacte");
    let img = doc.render_page(0, 1.0).unwrap();
    assert_eq!(img.width_px, (full.width_pt - 100.0).round() as u32);
    // Le texte « Page 1 » (72, 760) se retrouve en (22, 710) dans la page recadrée.
    let p = doc
        .text_boxes(0)
        .unwrap()
        .into_iter()
        .find(|b| b.ch == 'P')
        .unwrap();
    assert!(close(p.rect.x, 22.0, 3.0), "{:?}", p.rect);
    let reopened = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    assert_eq!(reopened.page_size(0).unwrap(), cropped);
    // Marges nulles = réinitialiser.
    doc.crop_pages(&CropSpec {
        pages: Some(vec![0]),
        margins: [0.0; 4],
    })
    .unwrap();
    assert_eq!(doc.page_size(0).unwrap(), full);
    // Marges qui ne laissent rien : refusées.
    assert!(matches!(
        doc.crop_pages(&CropSpec {
            pages: None,
            margins: [300.0, 0.0, 300.0, 0.0],
        }),
        Err(CoreError::InvalidOperation(_))
    ));
    assert!(
        doc.crop_pages(&CropSpec {
            pages: None,
            margins: [-1.0, 0.0, 0.0, 0.0],
        })
        .is_err()
    );
    assert_eq!(doc.page_size(0).unwrap(), full);
    // Seul le recadrage passé par `apply` est dans l'historique.
    doc.undo().unwrap();
    assert!(!doc.can_undo());
    assert_eq!(doc.page_size(0).unwrap(), full);
}

#[test]
fn overlays_and_crop_on_rotated_page_follow_the_screen() {
    let mut doc = Document::from_bytes(rotated_pdf(), None).unwrap();
    doc.add_page_numbers(&numbering("{n}", Corner::BottomLeft, 1))
        .unwrap();
    // Le chiffre est en bas à gauche **de l'écran** (page affichée 600 × 400).
    let digit = doc
        .text_boxes(0)
        .unwrap()
        .into_iter()
        .find(|b| b.ch == '1' && b.rect.x < 100.0)
        .expect("numéro en bas à gauche");
    assert!(close(digit.rect.x, 20.0, 3.0), "{:?}", digit.rect);
    assert!(close(digit.rect.y, 20.0, 4.0), "{:?}", digit.rect);
    assert!(digit.rect.w < digit.rect.h, "chiffre droit à l'écran");
    doc.add_watermark(&watermark("BROUILLON", None)).unwrap();
    assert!(page_text(&doc, 0).contains("BROUILLON"));
    // Rogner 100 pt à gauche de l'écran : la page affichée fait 500 × 400.
    doc.crop_pages(&CropSpec {
        pages: None,
        margins: [100.0, 0.0, 0.0, 0.0],
    })
    .unwrap();
    let size = doc.page_size(0).unwrap();
    assert!(
        close(size.width_pt, 500.0, 0.1) && close(size.height_pt, 400.0, 0.1),
        "{size:?}"
    );
}

// --- Couche de texte invisible (Lot 8, OCR) ---------------------------------------------

use mega_core::overlay::InvisibleWord;
use mega_core::testing::blank_pdf;
use mega_core::text::SearchOptions;

fn word(text: &str, x: f32, y: f32, w: f32, h: f32) -> InvisibleWord {
    InvisibleWord {
        text: text.to_string(),
        rect: mega_core::Rect { x, y, w, h },
    }
}

#[test]
fn invisible_words_are_extracted_and_searchable() {
    let mut doc = Document::from_bytes(blank_pdf(612.0, 792.0), None).unwrap();
    doc.add_invisible_words(
        0,
        &[
            word("Bonjour", 100.0, 600.0, 120.0, 20.0),
            word("monde", 240.0, 600.0, 90.0, 20.0),
        ],
    )
    .unwrap();
    let text = doc.page_text(0).unwrap();
    assert!(
        text.contains("Bonjour") && text.contains("monde"),
        "{text:?}"
    );
    let hits = doc
        .search_page(0, "monde", &SearchOptions::default())
        .unwrap();
    assert_eq!(hits.len(), 1);
    let r = hits[0].rects[0];
    // Le mot occupe sa boîte : début et fin à quelques points près, hauteur dans la ligne.
    assert!(close(r.x, 240.0, 3.0), "{r:?}");
    assert!(close(r.x + r.w, 330.0, 4.0), "{r:?}");
    assert!(r.y >= 595.0 && r.y + r.h <= 625.0, "{r:?}");
}

#[test]
fn invisible_words_are_not_rendered() {
    let mut doc = Document::from_bytes(blank_pdf(300.0, 200.0), None).unwrap();
    let before = doc.render_page(0, 1.0).unwrap().rgba;
    doc.add_invisible_words(0, &[word("Caché", 20.0, 100.0, 200.0, 40.0)])
        .unwrap();
    assert!(doc.page_text(0).unwrap().contains("Caché"));
    assert_eq!(doc.render_page(0, 1.0).unwrap().rgba, before);
}

#[test]
fn invisible_words_follow_a_rotated_page() {
    // Page affichée 600 × 400 (`/Rotate 90`) : le mot doit être là où on le voit à l'écran.
    let mut doc = Document::from_bytes(rotated_pdf(), None).unwrap();
    doc.add_invisible_words(0, &[word("Tourné", 300.0, 100.0, 150.0, 30.0)])
        .unwrap();
    let hits = doc
        .search_page(0, "Tourné", &SearchOptions::default())
        .unwrap();
    assert_eq!(hits.len(), 1);
    let r = hits[0].rects[0];
    assert!(close(r.x, 300.0, 4.0), "{r:?}");
    assert!(close(r.x + r.w, 450.0, 5.0), "{r:?}");
    assert!(r.w > r.h, "mot horizontal à l'écran : {r:?}");
}

#[test]
fn non_win_ansi_letters_are_kept() {
    let mut doc = Document::from_bytes(blank_pdf(300.0, 200.0), None).unwrap();
    doc.add_invisible_words(
        0,
        &[
            word("Ωmega", 20.0, 100.0, 100.0, 20.0),
            word("été", 150.0, 100.0, 60.0, 20.0),
        ],
    )
    .unwrap();
    let text = doc.page_text(0).unwrap();
    assert!(text.contains("Ωmega") && text.contains("été"), "{text:?}");
}

// --- Lot 9 : largeurs Helvetica (B24, C46) et MediaBox héritée (B29) --------------------

/// Les polices Helvetica que nous ajoutons déclarent leurs largeurs (`/Widths`).
#[test]
fn overlay_fonts_declare_widths() {
    // Page vierge : les seules polices sont les nôtres. Un document par chemin d'écriture
    // (filigrane, couche invisible).
    let mut marked = Document::from_bytes(blank_pdf(300.0, 300.0), None).unwrap();
    marked.add_watermark(&watermark("BROUILLON", None)).unwrap();
    let mut ocr = Document::from_bytes(blank_pdf(300.0, 300.0), None).unwrap();
    ocr.add_invisible_words(0, &[word("Mot", 20.0, 100.0, 60.0, 20.0)])
        .unwrap();
    for doc in [marked, ocr] {
        let saved = lopdf::Document::load_mem(&doc.to_bytes().unwrap()).unwrap();
        let fonts: Vec<&lopdf::Dictionary> = saved
            .objects
            .values()
            .filter_map(|o| o.as_dict().ok())
            .filter(|d| {
                d.get(b"BaseFont").ok() == Some(&lopdf::Object::Name(b"Helvetica".to_vec()))
            })
            .collect();
        assert_eq!(fonts.len(), 1);
        let font = fonts[0];
        assert_eq!(font.get(b"FirstChar").unwrap().as_i64().unwrap(), 32);
        assert_eq!(font.get(b"LastChar").unwrap().as_i64().unwrap(), 255);
        assert_eq!(font.get(b"Widths").unwrap().as_array().unwrap().len(), 224);
    }
}

/// Un mot OCR est étiré (`Tz`) d'après la **largeur d'avance** d'Helvetica, pas d'après
/// l'encre : « I » (278/1000) à 20 pt dans 50 pt de large → Tz = 100 × 50 / 5,56.
#[test]
fn ocr_word_box_matches_advance_width() {
    let mut doc = Document::from_bytes(blank_pdf(300.0, 300.0), None).unwrap();
    doc.add_invisible_words(0, &[word("I", 20.0, 100.0, 50.0, 20.0)])
        .unwrap();
    let saved = lopdf::Document::load_mem(&doc.to_bytes().unwrap()).unwrap();
    let page = *saved.get_pages().get(&1).unwrap();
    let content = lopdf::content::Content::decode(&saved.get_page_content(page)).unwrap();
    let tz = content
        .operations
        .iter()
        .find(|op| op.operator == "Tz")
        .and_then(|op| op.operands[0].as_float().ok())
        .unwrap();
    assert!((tz - 899.28).abs() < 1.0, "Tz = {tz}");
}

/// Une page qui hérite sa `/MediaBox` de `/Pages` (hors de l'origine) se recadre à partir
/// de cette boîte.
#[test]
fn inherited_media_box_is_read() {
    let mut doc =
        Document::from_bytes(mega_core::testing::inherited_media_box_pdf(), None).unwrap();
    doc.crop_pages(&CropSpec {
        margins: [10.0, 10.0, 10.0, 10.0],
        pages: None,
    })
    .unwrap();
    let saved = lopdf::Document::load_mem(&doc.to_bytes().unwrap()).unwrap();
    let page = *saved.get_pages().get(&1).unwrap();
    let crop: Vec<f32> = saved
        .get_dictionary(page)
        .unwrap()
        .get(b"CropBox")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_float().unwrap())
        .collect();
    assert_eq!(crop, vec![110.0, 110.0, 490.0, 690.0]);
}

/// C26 : le modèle anglais `{title}` vaut `{titre}`.
#[test]
fn numbering_accepts_title_placeholder() {
    let mut doc = Document::from_bytes(sample_pdf(1, "Report"), None).unwrap();
    doc.add_page_numbers(&numbering("{n} {title}", Corner::BottomCenter, 1))
        .unwrap();
    assert_eq!(page_text(&doc, 0), "Page11Report");
}

// --- Lot 10 : police Unicode incorporée -------------------------------------------------

/// Les polices d'un fichier, par sous-type (`Type1`, `Type0`…).
fn font_subtypes(doc: &Document) -> Vec<String> {
    let saved = lopdf::Document::load_mem(&doc.to_bytes().unwrap()).unwrap();
    saved
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .filter(|d| d.get(b"Type").ok() == Some(&lopdf::Object::Name(b"Font".to_vec())))
        .filter_map(|d| d.get(b"Subtype").ok()?.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .collect()
}

#[test]
fn watermark_with_greek_is_searchable() {
    let mut doc = Document::from_bytes(blank_pdf(595.0, 842.0), None).unwrap();
    doc.add_watermark(&watermark("Ελλάδα", None)).unwrap();
    let hits = doc
        .search_page(0, "Ελλάδα", &SearchOptions::default())
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert!(font_subtypes(&doc).contains(&"Type0".to_string()));
}

#[test]
fn ascii_watermark_still_uses_helvetica() {
    let mut doc = Document::from_bytes(blank_pdf(595.0, 842.0), None).unwrap();
    doc.add_watermark(&watermark("BROUILLON été", None))
        .unwrap();
    let fonts = font_subtypes(&doc);
    assert!(!fonts.iter().any(|f| f == "Type0"), "{fonts:?}");
}

/// Texte mixte (WinAnsi + cyrillique) : une seule passe en police Unicode, centrée.
#[test]
fn mixed_text_is_one_unicode_run() {
    let mut doc = Document::from_bytes(blank_pdf(600.0, 800.0), None).unwrap();
    let mut w = watermark("Prix : 10 € — Цена", None);
    w.angle_deg = 0.0;
    w.size = 30.0;
    doc.add_watermark(&w).unwrap();
    let hits = doc
        .search_page(0, "10 € — Цена", &SearchOptions::default())
        .unwrap();
    assert_eq!(hits.len(), 1, "texte mixte retrouvé d'un bloc");
    let boxes: Vec<_> = doc
        .text_boxes(0)
        .unwrap()
        .into_iter()
        .filter(|b| !b.ch.is_whitespace())
        .collect();
    let left = boxes.iter().map(|b| b.rect.x).fold(f32::MAX, f32::min);
    let right = boxes
        .iter()
        .map(|b| b.rect.x + b.rect.w)
        .fold(f32::MIN, f32::max);
    assert!(
        close((left + right) / 2.0, 300.0, 3.0),
        "centré : {left} … {right}"
    );
}

#[test]
fn page_numbers_in_cyrillic() {
    let mut doc = Document::from_bytes(sample_pdf(2, "n"), None).unwrap();
    doc.add_page_numbers(&numbering("Страница {n}", Corner::BottomCenter, 1))
        .unwrap();
    assert!(
        page_text(&doc, 1).contains("Страница2"),
        "{}",
        page_text(&doc, 1)
    );
}

#[test]
fn unicode_watermark_is_drawn() {
    let mut doc = Document::from_bytes(blank_pdf(300.0, 300.0), None).unwrap();
    let before = doc.render_page(0, 1.0).unwrap().rgba;
    let mut w = watermark("Привет", None);
    w.opacity = 1.0;
    w.color = Color { r: 0, g: 0, b: 0 };
    doc.add_watermark(&w).unwrap();
    let after = doc.render_page(0, 1.0).unwrap().rgba;
    let dark = after.chunks(4).filter(|p| p[0] < 100).count();
    assert!(
        after != before && dark > 200,
        "glyphes dessinés : {dark} pixels sombres"
    );
}

#[test]
fn ocr_layer_keeps_cyrillic() {
    let mut doc = Document::from_bytes(blank_pdf(300.0, 200.0), None).unwrap();
    doc.add_invisible_words(0, &[word("Привет", 20.0, 100.0, 100.0, 20.0)])
        .unwrap();
    assert!(doc.page_text(0).unwrap().contains("Привет"));
}

#[test]
fn ocr_layer_keeps_unmapped_glyphs_searchable() {
    let mut doc = Document::from_bytes(blank_pdf(300.0, 200.0), None).unwrap();
    doc.add_invisible_words(0, &[word("你好", 20.0, 100.0, 60.0, 20.0)])
        .unwrap();
    let hits = doc
        .search_page(0, "你好", &SearchOptions::default())
        .unwrap();
    assert_eq!(hits.len(), 1);
}

/// Mineur du Lot 9 : une boîte de mot infinie (OCR fautif) est ignorée, elle n'écrit pas
/// « inf » dans le flux de contenu.
#[test]
fn infinite_word_boxes_are_skipped() {
    let mut doc = Document::from_bytes(blank_pdf(300.0, 200.0), None).unwrap();
    doc.add_invisible_words(
        0,
        &[
            word("Infini", 20.0, 100.0, f32::INFINITY, 20.0),
            word("Haut", 20.0, 100.0, 50.0, f32::INFINITY),
            word("Normal", 20.0, 50.0, 80.0, 20.0),
        ],
    )
    .unwrap();
    let saved = lopdf::Document::load_mem(&doc.to_bytes().unwrap()).unwrap();
    let page = *saved.get_pages().get(&1).unwrap();
    let content = String::from_utf8_lossy(&saved.get_page_content(page)).into_owned();
    assert!(!content.contains("inf"), "{content}");
    let text = doc.page_text(0).unwrap();
    assert!(
        text.contains("Normal") && !text.contains("Infini"),
        "{text:?}"
    );
}

/// Relecture du Lot 10 (C1) : deux passes Unicode sur la même page (filigrane, numéros,
/// couche OCR) ont chacune leur police ; la seconde ne doit pas réinterpréter la première.
#[test]
fn chained_unicode_overlays_keep_every_text() {
    let mut doc = Document::from_bytes(blank_pdf(400.0, 400.0), None).unwrap();
    doc.add_invisible_words(0, &[word("Договор", 20.0, 300.0, 120.0, 20.0)])
        .unwrap();
    let mut w = watermark("ЧЕРНОВИК", None);
    w.angle_deg = 0.0;
    doc.add_watermark(&w).unwrap();
    doc.add_page_numbers(&numbering("Стр. {n}", Corner::BottomCenter, 1))
        .unwrap();
    let text = doc.page_text(0).unwrap();
    for expected in ["Договор", "ЧЕРНОВИК", "Стр. 1"] {
        assert!(text.contains(expected), "{expected} absent de {text:?}");
    }
}

// --- Lot 14 : artefacts, /CIDSet, groupe de transparence (audit E6) ----------------------

/// Contenu décodé de la première page du fichier enregistré.
fn first_page_content(doc: &Document) -> String {
    let saved = lopdf::Document::load_mem(&doc.to_bytes().unwrap()).unwrap();
    let id = *saved.get_pages().get(&1).unwrap();
    String::from_utf8_lossy(&saved.get_page_content(id)).into_owned()
}

/// Le contenu d'origine reste hors de l'artefact ; le texte ajouté est entre `/Artifact`
/// (`BMC`, ou `BDC` avec des propriétés) et `EMC` (PDF/UA-1 §7.1).
fn assert_added_is_artifact(content: &str, original: &str, added: &str) {
    let bmc = content.find("/Artifact").expect("/Artifact absent");
    let emc = content.rfind("EMC").expect("EMC absent");
    assert!(bmc < emc, "{content}");
    let orig = content.find(original).expect("contenu d'origine perdu");
    assert!(orig < bmc, "le contenu d'origine doit précéder l'artefact");
    let at = bmc + content[bmc..].find(added).expect("contenu ajouté absent");
    assert!(bmc < at && at < emc, "{content}");
}

#[test]
fn watermark_content_is_marked_as_artifact() {
    let mut doc = Document::from_bytes(sample_pdf(1, "filigrane"), None).unwrap();
    doc.add_watermark(&watermark("BROUILLON", None)).unwrap();
    assert_added_is_artifact(&first_page_content(&doc), "TJ", "/MegaGS");
}

#[test]
fn page_numbers_and_ocr_layer_are_marked_as_artifacts() {
    let mut numbered = Document::from_bytes(sample_pdf(1, "numéros"), None).unwrap();
    numbered
        .add_page_numbers(&numbering("{n}", Corner::BottomCenter, 1))
        .unwrap();
    assert_added_is_artifact(&first_page_content(&numbered), "TJ", "Tf");
    let content = first_page_content(&numbered);
    // Un numéro de page est un artefact de pagination (ISO 32000-1 §14.8.2.2.2).
    assert_eq!(
        content
            .matches("/Artifact <</Type /Pagination>> BDC")
            .count(),
        1,
        "{content}"
    );
    assert!(!content.contains("/Artifact BMC"), "{content}");

    let mut ocr = Document::from_bytes(sample_pdf(1, "ocr"), None).unwrap();
    ocr.add_invisible_words(0, &[word("Mot", 20.0, 100.0, 60.0, 20.0)])
        .unwrap();
    let content = first_page_content(&ocr);
    let bmc = content.find("/Artifact BMC").expect("/Artifact BMC absent");
    assert!(content.find("3 Tr").unwrap() > bmc, "{content}");
    assert!(content.rfind("EMC").unwrap() > content.find("3 Tr").unwrap());
}

#[test]
fn unicode_subset_has_a_cidset() {
    let mut doc = Document::from_bytes(blank_pdf(595.0, 842.0), None).unwrap();
    doc.add_watermark(&watermark("ΩΣΔ", None)).unwrap();
    let saved = lopdf::Document::load_mem(&doc.to_bytes().unwrap()).unwrap();
    let descriptor = saved
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .find(|d| {
            d.get(b"Type").ok() == Some(&lopdf::Object::Name(b"FontDescriptor".to_vec()))
                && d.has(b"FontFile2")
        })
        .expect("descripteur de la police Unicode");
    let id = descriptor.get(b"CIDSet").unwrap().as_reference().unwrap();
    let stream = saved.get_object(id).unwrap().as_stream().unwrap();
    let bits = stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone());
    // Trois caractères distincts : CID 1 à 3 (le CID 0, .notdef, est aussi marqué).
    assert_eq!(bits.len(), 1, "ceil((3+1)/8) octets");
    for cid in 0..=3usize {
        assert_ne!(bits[cid / 8] & (0x80 >> (cid % 8)), 0, "CID {cid}");
    }
}

fn page_group(doc: &Document) -> Option<lopdf::Dictionary> {
    let saved = lopdf::Document::load_mem(&doc.to_bytes().unwrap()).unwrap();
    let id = *saved.get_pages().get(&1).unwrap();
    let page = saved.get_dictionary(id).unwrap();
    let group = page.get(b"Group").ok()?;
    saved.dereference(group).ok()?.1.as_dict().ok().cloned()
}

#[test]
fn translucent_watermark_adds_a_page_transparency_group() {
    let mut doc = Document::from_bytes(sample_pdf(1, "groupe"), None).unwrap();
    assert!(page_group(&doc).is_none());
    doc.add_watermark(&watermark("BROUILLON", None)).unwrap();
    let group = page_group(&doc).expect("/Group posé");
    assert_eq!(group.get(b"S").unwrap().as_name().unwrap(), b"Transparency");
    assert_eq!(group.get(b"CS").unwrap().as_name().unwrap(), b"DeviceRGB");

    // Opaque : aucun groupe.
    let mut opaque = Document::from_bytes(sample_pdf(1, "groupe"), None).unwrap();
    let mut w = watermark("BROUILLON", None);
    w.opacity = 1.0;
    opaque.add_watermark(&w).unwrap();
    assert!(page_group(&opaque).is_none());
}

#[test]
fn an_existing_page_group_is_kept() {
    let mut doc = Document::from_bytes(sample_pdf(1, "groupe"), None).unwrap();
    doc.add_watermark(&watermark("UN", None)).unwrap();
    // Le groupe existe désormais : le second filigrane le laisse intact.
    let before = page_group(&doc).unwrap();
    doc.add_watermark(&watermark("DEUX", None)).unwrap();
    assert_eq!(page_group(&doc).unwrap(), before);
}

#[test]
fn pdfa_documents_get_an_embedded_font_even_for_latin_text() {
    let mut pdfa = Document::from_bytes(mega_core::testing::pdfa_pdf(), None).unwrap();
    pdfa.add_watermark(&watermark("BROUILLON", None)).unwrap();
    let saved = lopdf::Document::load_mem(&pdfa.to_bytes().unwrap()).unwrap();
    let embedded = saved
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .any(|d| {
            d.has(b"FontFile2")
                && d.get(b"FontName")
                    .ok()
                    .and_then(|n| n.as_name().ok())
                    .is_some_and(|n| n.len() > 7 && n[6] == b'+')
        });
    assert!(embedded, "police sous-ensemble incorporée attendue");
    assert!(font_subtypes(&pdfa).contains(&"Type0".to_string()));

    let mut plain = Document::open(fixture("simple.pdf"), None).unwrap();
    plain.add_watermark(&watermark("BROUILLON", None)).unwrap();
    assert!(!font_subtypes(&plain).contains(&"Type0".to_string()));
}

// --- Lot 14, partie C : couleur et transparence sur un document PDF/A -------------------

/// Chemin d'un fichier de `fixtures/`, indépendant du dossier courant.
fn fixture(name: &str) -> String {
    format!("{}/../../fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

/// Dernier flux de contenu de la première page (celui que nous venons d'ajouter), décodé, et
/// les ressources `/ColorSpace` de la page.
fn added_content_and_color_spaces(doc: &Document) -> (String, Option<lopdf::Dictionary>) {
    let saved = lopdf::Document::load_mem(&doc.to_bytes().unwrap()).unwrap();
    let id = *saved.get_pages().get(&1).unwrap();
    let last = *saved.get_page_contents(id).last().unwrap();
    let stream = saved.get_object(last).unwrap().as_stream().unwrap();
    let content = stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone());
    let page = saved.get_dictionary(id).unwrap();
    let resources = saved
        .dereference(page.get(b"Resources").unwrap())
        .unwrap()
        .1
        .as_dict()
        .unwrap()
        .clone();
    let spaces = resources
        .get(b"ColorSpace")
        .ok()
        .and_then(|c| saved.dereference(c).ok())
        .and_then(|(_, c)| c.as_dict().ok().cloned())
        .map(|spaces| {
            // Chaque entrée déréférencée, pour lire le tableau `[/CalRGB << … >>]`.
            let mut out = lopdf::Dictionary::new();
            for (name, value) in spaces.iter() {
                out.set(name.clone(), saved.dereference(value).unwrap().1.clone());
            }
            out
        });
    (String::from_utf8_lossy(&content).into_owned(), spaces)
}

/// Les trois composantes de la couleur posée par `sc` (ou `rg`) dans `content`.
fn fill_components(content: &str, operator: &str) -> [f32; 3] {
    let line = content
        .lines()
        .find(|l| l.trim_end().ends_with(&format!(" {operator}")) && !l.contains(" cs "))
        .or_else(|| {
            content
                .lines()
                .find(|l| l.trim_end().ends_with(&format!(" {operator}")))
        })
        .unwrap_or_else(|| panic!("opérateur {operator} absent : {content}"));
    let numbers: Vec<f32> = line
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    let n = numbers.len();
    [numbers[n - 3], numbers[n - 2], numbers[n - 1]]
}

/// PDF/A (toutes parties) : la couleur du texte ajouté s'écrit dans un espace indépendant du
/// périphérique (CalRGB), pas en DeviceRGB (`rg`), interdit sans profil de sortie RVB
/// (PDF/A-2 6.2.4.3).
#[test]
fn pdfa_overlays_use_a_calibrated_color_space() {
    let mut pdfa = Document::from_bytes(mega_core::testing::pdfa_pdf(), None).unwrap();
    pdfa.add_watermark(&watermark("BROUILLON", None)).unwrap();
    let (content, spaces) = added_content_and_color_spaces(&pdfa);
    assert!(content.contains(" cs"), "{content}");
    assert!(content.contains(" sc"), "{content}");
    assert!(!content.contains(" rg"), "{content}");
    let spaces = spaces.expect("/ColorSpace dans les ressources de la page");
    let (name, space) = spaces.iter().next().unwrap();
    assert!(
        content.contains(&format!("/{} cs", String::from_utf8_lossy(name))),
        "{content}"
    );
    let space = space.as_array().unwrap();
    assert_eq!(space[0].as_name().unwrap(), b"CalRGB");
    let white = space[1]
        .as_dict()
        .unwrap()
        .get(b"WhitePoint")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(white.len(), 3);
    // Le groupe de transparence fusionne dans ce même espace.
    let group = page_group(&pdfa).expect("/Group posé (opacité 0,3)");
    let cs = group.get(b"CS").unwrap().as_array().unwrap();
    assert_eq!(cs[0].as_name().unwrap(), b"CalRGB");

    // Les numéros de page aussi.
    let mut numbered = Document::from_bytes(mega_core::testing::pdfa_pdf(), None).unwrap();
    numbered
        .add_page_numbers(&numbering("{n}", Corner::BottomCenter, 1))
        .unwrap();
    let (content, _) = added_content_and_color_spaces(&numbered);
    assert!(
        content.contains(" sc") && !content.contains(" rg"),
        "{content}"
    );

    // Un document ordinaire garde `rg` (DeviceRGB).
    let mut plain = Document::open(fixture("simple.pdf"), None).unwrap();
    plain.add_watermark(&watermark("BROUILLON", None)).unwrap();
    let (content, spaces) = added_content_and_color_spaces(&plain);
    assert!(content.contains(" rg"), "{content}");
    assert!(!content.contains(" sc"), "{content}");
    assert!(spaces.is_none_or(|s| !s.iter().any(|(n, _)| n.starts_with(b"MegaCS"))));
}

/// PDF/A-1 interdit toute transparence (6.4) : ni `/Group`, ni `/CA`/`/ca` < 1 ; l'opacité
/// est simulée en éclaircissant la couleur vers le blanc.
#[test]
fn pdfa1_watermarks_simulate_opacity_without_transparency() {
    let mut doc = Document::from_bytes(mega_core::testing::pdfa_part_pdf(1), None).unwrap();
    doc.add_watermark(&watermark("BROUILLON", None)).unwrap();
    assert!(page_group(&doc).is_none(), "PDF/A-1 : aucun /Group");
    let saved = lopdf::Document::load_mem(&doc.to_bytes().unwrap()).unwrap();
    for state in saved
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .filter(|d| d.get(b"Type").ok() == Some(&lopdf::Object::Name(b"ExtGState".to_vec())))
    {
        for key in [&b"CA"[..], b"ca"] {
            if let Ok(v) = state.get(key) {
                assert!(v.as_float().unwrap() >= 1.0, "{state:?}");
            }
        }
    }
    let (content, _) = added_content_and_color_spaces(&doc);
    // Gris 128 à 30 % sur du blanc : 128/255 × 0,3 + 0,7 ≈ 0,851.
    let [r, g, b] = fill_components(&content, "sc");
    for c in [r, g, b] {
        assert!((c - 0.851).abs() < 0.01, "couleur éclaircie : {content}");
    }
}

/// Une page hors bornes est refusée **avant** que le document ne soit marqué modifié
/// (CLAUDE.md : `mark_changed` après les gardes de refus) : enregistrer réécrit alors les
/// octets d'origine tels quels.
#[test]
fn invisible_words_on_a_missing_page_leave_the_document_unchanged() {
    let source = sample_pdf(1, "intact");
    let mut doc = Document::from_bytes(source.clone(), None).unwrap();
    let word = mega_core::overlay::InvisibleWord {
        text: "Reconnu".to_string(),
        rect: mega_core::Rect {
            x: 100.0,
            y: 600.0,
            w: 80.0,
            h: 14.0,
        },
    };
    assert!(matches!(
        doc.add_invisible_words(5, &[word]),
        Err(CoreError::PageOutOfRange { index: 5, count: 1 })
    ));
    assert!(!doc.is_modified(), "un refus ne modifie pas le document");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("intact.pdf");
    doc.save_as(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), source, "octets d'origine");
}

/// Enregistre `doc` et rend les octets écrits.
fn saved_bytes(doc: &mut Document) -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sortie.pdf");
    doc.save_as(&path).unwrap();
    std::fs::read(&path).unwrap()
}

/// Un filigrane refusé par ses gardes (texte vide, page absente, taille hors bornes) laisse
/// le document **non modifié** : enregistrer réécrit les octets d'origine tels quels.
#[test]
fn a_refused_watermark_leaves_the_document_unchanged() {
    let source = sample_pdf(1, "intact");
    let too_big = Watermark {
        size: 1000.0,
        ..watermark("GRAND", None)
    };
    for (what, spec) in [
        ("texte vide", watermark("   ", None)),
        ("page absente", watermark("COPIE", Some(vec![5]))),
        ("taille hors bornes", too_big),
    ] {
        let mut doc = Document::from_bytes(source.clone(), None).unwrap();
        assert!(doc.add_watermark(&spec).is_err(), "{what} : refusé");
        assert!(!doc.is_modified(), "{what} : document non modifié");
        assert!(saved_bytes(&mut doc) == source, "{what} : octets d'origine");
    }
}

/// Même règle pour la numérotation, y compris quand le refus vient de la boucle qui place
/// chaque numéro (marge qui sort de la page).
#[test]
fn refused_page_numbers_leave_the_document_unchanged() {
    let source = sample_pdf(2, "intact");
    let off_page = PageNumbering {
        margin: 5000.0,
        ..numbering("{n}", Corner::BottomRight, 1)
    };
    for (what, spec) in [
        ("modèle vide", numbering("  ", Corner::BottomCenter, 1)),
        ("marge hors de la page", off_page),
    ] {
        let mut doc = Document::from_bytes(source.clone(), None).unwrap();
        assert!(doc.add_page_numbers(&spec).is_err(), "{what} : refusé");
        assert!(!doc.is_modified(), "{what} : document non modifié");
        assert!(saved_bytes(&mut doc) == source, "{what} : octets d'origine");
    }
}

/// Un recadrage refusé ne touche **aucune** page, même quand le refus vient d'une page
/// plus loin dans la liste : `two_level_pages_pdf` a une page 1 de 600 pt de large (affichée)
/// et une page 3 de 595 pt ; 597 pt de marges laissent 3 pt à la première et rien à la
/// seconde. Toutes les pages sont vérifiées avant d'en recadrer une.
#[test]
fn a_refused_crop_leaves_every_page_untouched() {
    let source = mega_core::testing::two_level_pages_pdf();
    for (what, spec) in [
        (
            "marges trop grandes pour la page 3",
            CropSpec {
                margins: [0.0, 0.0, 597.0, 0.0],
                pages: Some(vec![0, 2]),
            },
        ),
        (
            "page absente",
            CropSpec {
                margins: [10.0, 10.0, 10.0, 10.0],
                pages: Some(vec![0, 9]),
            },
        ),
        (
            "marge négative",
            CropSpec {
                margins: [-1.0, 0.0, 0.0, 0.0],
                pages: None,
            },
        ),
    ] {
        let mut doc = Document::from_bytes(source.clone(), None).unwrap();
        assert!(doc.crop_pages(&spec).is_err(), "{what} : refusé");
        let lo = lopdf::Document::load_mem(&doc.to_bytes().unwrap()).unwrap();
        let first = lo.get_dictionary(lo.get_pages()[&1]).unwrap();
        assert!(!first.has(b"CropBox"), "{what} : page 1 recadrée");
        assert!(saved_bytes(&mut doc) == source, "{what} : octets d'origine");
    }
}
