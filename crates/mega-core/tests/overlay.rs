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
    w.text = "Σ".to_string();
    assert!(doc.add_watermark(&w).is_err(), "hors WinAnsi");
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
    assert_eq!(page_text(&doc, 0), "Page1p.5");
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
