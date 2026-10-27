// SPDX-License-Identifier: GPL-3.0-or-later
//! Formulaires : lecture des champs, remplissage, aplatissement, signature visuelle.

use mega_core::testing::{
    form_parent_kid_pdf, form_pdf, form_pdf_without_need_appearances, sample_pdf, xfa_pdf,
};
use mega_core::{CoreError, Document, FieldKind, FieldValue, FormType};

fn form() -> Document {
    Document::from_bytes(form_pdf(), None).unwrap()
}

#[test]
fn reads_form_fields() {
    let doc = form();
    let fields = doc.form_fields(0).unwrap();
    let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "nom", "adresse", "ok", "choix", "choix", "ville", "ref", "email"
        ]
    );
    let kinds: Vec<FieldKind> = fields.iter().map(|f| f.kind).collect();
    assert_eq!(
        kinds,
        vec![
            FieldKind::Text,
            FieldKind::Text,
            FieldKind::Checkbox,
            FieldKind::Radio,
            FieldKind::Radio,
            FieldKind::Combo,
            FieldKind::Text,
            FieldKind::Text
        ]
    );
    assert_eq!(fields[0].index, 0);
    assert_eq!(fields[0].page, 0);
    assert_eq!(fields[0].value.as_deref(), Some(""));
    assert!(fields[1].multiline);
    assert_eq!(fields[2].checked, Some(false));
    assert_eq!(fields[3].checked, Some(true), "radio A coché");
    assert_eq!(fields[4].checked, Some(false), "radio B");
    let ville = &fields[5];
    assert_eq!(ville.value.as_deref(), Some("Paris"));
    assert_eq!(ville.options.len(), 2);
    assert_eq!(ville.options[0].label, "Paris");
    assert!(ville.options[0].selected && !ville.options[1].selected);
    assert!(fields[6].read_only && fields[6].value.as_deref() == Some("REF-001"));
    assert!(fields[7].required && !fields[7].read_only);
    assert!((fields[0].rect.x - 72.0).abs() < 0.5 && (fields[0].rect.w - 228.0).abs() < 0.5);
}

#[test]
fn form_info_detects_acroform_and_xfa() {
    let info = form().form_info().unwrap();
    assert_eq!(info.form_type, FormType::AcroForm);
    assert_eq!(
        info.field_count, 8,
        "8 widgets (le groupe radio en compte 2)"
    );
    let plain = Document::from_bytes(sample_pdf(1, "sans"), None).unwrap();
    let info = plain.form_info().unwrap();
    assert_eq!(info.form_type, FormType::None);
    assert_eq!(info.field_count, 0);
    let xfa = Document::from_bytes(xfa_pdf(), None).unwrap();
    assert_eq!(xfa.form_info().unwrap().form_type, FormType::Xfa);
}

#[test]
fn form_fields_on_page_without_widgets_is_empty() {
    let plain = Document::from_bytes(sample_pdf(2, "sans"), None).unwrap();
    assert!(plain.form_fields(1).unwrap().is_empty());
    assert!(plain.form_fields(5).is_err());
}

// ---- Remplissage, aplatissement (Task 2) ----

fn text(s: &str) -> FieldValue {
    FieldValue::Text {
        text: s.to_string(),
    }
}

fn reopen(doc: &Document) -> Document {
    Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap()
}

#[test]
fn set_text_value_round_trips_and_renders() {
    let mut doc = form();
    let before = doc.render_page(0, 1.0).unwrap();
    doc.set_field_value(0, 0, &text("Dupont")).unwrap();
    assert_eq!(
        doc.form_fields(0).unwrap()[0].value.as_deref(),
        Some("Dupont")
    );
    let after = doc.render_page(0, 1.0).unwrap();
    assert_ne!(
        before.rgba, after.rgba,
        "l'apparence du champ suit la valeur"
    );
    let again = reopen(&doc);
    assert_eq!(
        again.form_fields(0).unwrap()[0].value.as_deref(),
        Some("Dupont")
    );
    assert_eq!(again.render_page(0, 1.0).unwrap().rgba, after.rgba);
}

#[test]
fn check_box_and_radio_group() {
    let mut doc = form();
    let before = doc.render_page(0, 1.0).unwrap();
    doc.set_field_value(0, 2, &FieldValue::Checked { checked: true })
        .unwrap();
    let fields = reopen(&doc).form_fields(0).unwrap();
    assert_eq!(fields[2].checked, Some(true));
    // La valeur vient de PDFium lui-même (`FPDFAnnot_GetFormFieldValue`) : c'est le **nom**
    // d'état, pas une chaîne « /Yes » (défaut de pdfium-render contourné par lopdf).
    assert_eq!(fields[2].value.as_deref(), Some("Yes"));
    let bytes = doc.to_bytes().unwrap();
    let text_bytes = String::from_utf8_lossy(&bytes);
    assert!(
        text_bytes.contains("/AS/Yes") || text_bytes.contains("/AS /Yes"),
        "/AS est un nom"
    );
    assert!(!text_bytes.contains("(/Yes)"), "jamais la chaîne « /Yes »");
    assert_ne!(
        doc.render_page(0, 1.0).unwrap().rgba,
        before.rgba,
        "coche visible"
    );
    doc.set_field_value(0, 2, &FieldValue::Checked { checked: false })
        .unwrap();
    assert_eq!(doc.form_fields(0).unwrap()[2].checked, Some(false));
    // Radio : cocher B décoche A (le `/V` du groupe et les `/AS` des deux boutons bougent).
    doc.set_field_value(0, 4, &FieldValue::Checked { checked: true })
        .unwrap();
    let fields = reopen(&doc).form_fields(0).unwrap();
    assert_eq!(fields[3].checked, Some(false), "A décoché");
    assert_eq!(fields[4].checked, Some(true), "B coché");
    assert_eq!(fields[4].value.as_deref(), Some("B"));
    assert!(matches!(
        doc.set_field_value(0, 4, &FieldValue::Checked { checked: false }),
        Err(CoreError::InvalidOperation(_))
    ));
}

#[test]
fn combo_value_is_written() {
    let mut doc = form();
    doc.set_field_value(0, 5, &text("Lyon")).unwrap();
    let f = &reopen(&doc).form_fields(0).unwrap()[5];
    assert_eq!(f.value.as_deref(), Some("Lyon"));
    assert!(
        matches!(
            doc.set_field_value(0, 5, &text("Marseille")),
            Err(CoreError::InvalidOperation(_))
        ),
        "une valeur hors des options est refusée"
    );
}

#[test]
fn parent_kid_text_field_is_filled() {
    let mut doc = Document::from_bytes(form_parent_kid_pdf(), None).unwrap();
    assert_eq!(
        doc.form_fields(0).unwrap()[0].value.as_deref(),
        Some("ancien")
    );
    doc.set_field_value(0, 0, &text("Dupont")).unwrap();
    assert_eq!(
        doc.form_fields(0).unwrap()[0].value.as_deref(),
        Some("Dupont")
    );
    assert_eq!(
        reopen(&doc).form_fields(0).unwrap()[0].value.as_deref(),
        Some("Dupont")
    );
}

#[test]
fn long_text_is_refused() {
    let mut doc = form();
    let long = "x".repeat(5000);
    assert!(matches!(
        doc.set_field_value(0, 0, &text(&long)),
        Err(CoreError::InvalidOperation(_))
    ));
}

#[test]
fn read_only_field_is_refused() {
    let mut doc = form();
    assert!(matches!(
        doc.set_field_value(0, 6, &text("autre")),
        Err(CoreError::InvalidOperation(_))
    ));
    assert_eq!(
        doc.form_fields(0).unwrap()[6].value.as_deref(),
        Some("REF-001")
    );
}

#[test]
fn wrong_value_kind_is_refused() {
    let mut doc = form();
    // Texte sur une case, coche sur un texte, valeur sur un combo (non modifiable au Lot 4).
    assert!(matches!(
        doc.set_field_value(0, 2, &text("x")),
        Err(CoreError::InvalidOperation(_))
    ));
    assert!(matches!(
        doc.set_field_value(0, 0, &FieldValue::Checked { checked: true }),
        Err(CoreError::InvalidOperation(_))
    ));
    assert!(matches!(
        doc.set_field_value(0, 5, &FieldValue::Checked { checked: true }),
        Err(CoreError::InvalidOperation(_))
    ));
    assert!(matches!(
        doc.set_field_value(0, 9, &text("x")),
        Err(CoreError::InvalidOperation(_))
    ));
    assert!(matches!(
        doc.set_field_value(3, 0, &text("x")),
        Err(CoreError::PageOutOfRange { .. })
    ));
}

#[test]
fn value_is_written_without_need_appearances() {
    let mut doc = Document::from_bytes(form_pdf_without_need_appearances(), None).unwrap();
    doc.set_field_value(0, 0, &text("Durand")).unwrap();
    let again = reopen(&doc);
    assert_eq!(
        again.form_fields(0).unwrap()[0].value.as_deref(),
        Some("Durand")
    );
    // L'apparence PDFium n'est pas forcément régénérée ici : seule la valeur est garantie.
}

fn page_text(doc: &Document, page: u16) -> String {
    doc.text_boxes(page)
        .unwrap()
        .iter()
        .map(|b| b.ch)
        .filter(|c| !c.is_whitespace())
        .collect()
}

#[test]
fn value_is_visible_without_need_appearances_and_survives_flatten() {
    let mut doc = Document::from_bytes(form_pdf_without_need_appearances(), None).unwrap();
    let before = doc.render_page(0, 1.0).unwrap();
    doc.set_field_value(0, 0, &text("Durand")).unwrap();
    // `/NeedAppearances true` est posé à l'écriture : PDFium régénère l'apparence au
    // rechargement, la valeur est visible chez nous et ailleurs.
    assert_ne!(doc.render_page(0, 1.0).unwrap().rgba, before.rgba);
    // Aplatir sans rendu préalable (cas du CLI) conserve la valeur dans le contenu.
    let mut doc = Document::from_bytes(form_pdf_without_need_appearances(), None).unwrap();
    doc.set_field_value(0, 0, &text("Durand")).unwrap();
    doc.flatten_all().unwrap();
    assert!(
        page_text(&doc, 0).contains("Durand"),
        "{}",
        page_text(&doc, 0)
    );
}

#[test]
fn flatten_removes_fields_and_keeps_rendering() {
    let mut doc = form();
    doc.set_field_value(0, 0, &text("Dupont")).unwrap();
    let before = doc.render_page(0, 1.0).unwrap();
    doc.flatten_all().unwrap();
    assert!(doc.form_fields(0).unwrap().is_empty());
    assert_eq!(doc.form_info().unwrap().field_count, 0);
    let after = doc.render_page(0, 1.0).unwrap();
    let white = after
        .rgba
        .chunks(4)
        .all(|p| p[0] == 255 && p[1] == 255 && p[2] == 255);
    assert!(!white, "le contenu aplati reste visible");
    let _ = before;
    let again = reopen(&doc);
    assert!(again.form_fields(0).unwrap().is_empty());
    // Le dictionnaire /AcroForm est retiré : plus de champs orphelins pour les autres lecteurs.
    assert_eq!(again.form_info().unwrap().form_type, FormType::None);
    assert!(
        page_text(&again, 0).contains("Dupont"),
        "la valeur saisie est dans le contenu"
    );
    // Sans formulaire : succès sans changement.
    let mut plain = Document::from_bytes(sample_pdf(1, "plat"), None).unwrap();
    plain.flatten_all().unwrap();
    assert_eq!(plain.page_count(), 1);
}

// ---- Signature visuelle (Task 3) ----

use mega_core::{Rect, SignatureSource};

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, w, h }
}

#[test]
fn signature_strokes_change_rendering_and_survive_reopen() {
    let mut doc = Document::from_bytes(sample_pdf(1, "sig"), None).unwrap();
    let before = doc.render_page(0, 1.0).unwrap();
    doc.insert_signature(
        0,
        rect(100.0, 100.0, 200.0, 60.0),
        &SignatureSource::Strokes {
            strokes: vec![
                vec![[100.0, 110.0], [150.0, 150.0], [200.0, 110.0]],
                vec![[210.0, 120.0], [290.0, 140.0]],
            ],
        },
    )
    .unwrap();
    let after = doc.render_page(0, 1.0).unwrap();
    assert_ne!(before.rgba, after.rgba);
    let reopened = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    assert_eq!(reopened.render_page(0, 1.0).unwrap().rgba, after.rgba);
    // Ce n'est pas une annotation : rien dans /Annots.
    assert!(reopened.annotations(0).unwrap().is_empty());
}

#[test]
fn signature_image_is_fitted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sig.png");
    // Image 20×10 (ratio 2:1), placée dans un carré 100×100 → 100×50 centré.
    let img = image::RgbaImage::from_fn(20, 10, |x, _| {
        if x < 10 {
            image::Rgba([0, 0, 0, 255])
        } else {
            image::Rgba([255, 0, 0, 255])
        }
    });
    img.save(&path).unwrap();
    let mut doc = Document::from_bytes(sample_pdf(1, "img"), None).unwrap();
    let before = doc.render_page(0, 1.0).unwrap();
    doc.insert_signature(
        0,
        rect(50.0, 400.0, 100.0, 100.0),
        &SignatureSource::Image {
            path: path.to_string_lossy().into_owned(),
        },
    )
    .unwrap();
    let after = doc.render_page(0, 1.0).unwrap();
    assert_ne!(before.rgba, after.rgba);
    let placed = doc.last_image_bounds(0).unwrap().expect("un objet image");
    assert!(
        (placed.w - 100.0).abs() < 1.0 && (placed.h - 50.0).abs() < 1.0,
        "{placed:?}"
    );
    assert!(
        (placed.x - 50.0).abs() < 1.0 && (placed.y - 425.0).abs() < 1.0,
        "{placed:?}"
    );
}

#[test]
fn corrupt_or_huge_image_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let text_file = dir.path().join("pas-une-image.png");
    std::fs::write(&text_file, b"bonjour").unwrap();
    let mut doc = Document::from_bytes(sample_pdf(1, "bad"), None).unwrap();
    let before = doc.render_page(0, 1.0).unwrap();
    assert!(matches!(
        doc.insert_signature(
            0,
            rect(0.0, 0.0, 50.0, 50.0),
            &SignatureSource::Image {
                path: text_file.to_string_lossy().into_owned()
            }
        ),
        Err(CoreError::InvalidOperation(_))
    ));
    let huge = dir.path().join("huge.png");
    image::RgbaImage::new(9000, 10).save(&huge).unwrap();
    assert!(matches!(
        doc.insert_signature(
            0,
            rect(0.0, 0.0, 50.0, 50.0),
            &SignatureSource::Image {
                path: huge.to_string_lossy().into_owned()
            }
        ),
        Err(CoreError::InvalidOperation(_))
    ));
    assert!(matches!(
        doc.insert_signature(
            0,
            rect(0.0, 0.0, 50.0, 50.0),
            &SignatureSource::Strokes { strokes: vec![] }
        ),
        Err(CoreError::InvalidOperation(_))
    ));
    assert!(matches!(
        doc.insert_signature(
            0,
            rect(0.0, 0.0, 0.0, 50.0),
            &SignatureSource::Strokes {
                strokes: vec![vec![[0.0, 0.0], [1.0, 1.0]]]
            }
        ),
        Err(CoreError::InvalidOperation(_))
    ));
    assert_eq!(
        doc.render_page(0, 1.0).unwrap().rgba,
        before.rgba,
        "document inchangé"
    );
}

/// Séquence de l'application : rendu (environnement de formulaire actif), coche, rendu, relecture.
#[test]
fn checkbox_stays_checked_across_renders() {
    let mut doc = form();
    doc.render_page(0, 1.0).unwrap();
    doc.set_field_value(0, 2, &FieldValue::Checked { checked: true })
        .unwrap();
    doc.render_page(0, 1.0).unwrap();
    assert_eq!(doc.form_fields(0).unwrap()[2].checked, Some(true));
    doc.set_field_value(0, 0, &text("Dupont")).unwrap();
    doc.render_page(0, 1.0).unwrap();
    let fields = doc.form_fields(0).unwrap();
    assert_eq!(fields[0].value.as_deref(), Some("Dupont"));
    assert_eq!(fields[2].checked, Some(true));
}

/// Une signature de plusieurs traits n'ajoute qu'un flux de contenu à la page.
#[test]
fn signature_strokes_add_one_content_stream() {
    let count_streams = |bytes: &[u8]| -> usize {
        let doc = lopdf::Document::load_mem(bytes).unwrap();
        let (_, page_id) = doc.get_pages().into_iter().next().unwrap();
        let page = doc.get_dictionary(page_id).unwrap();
        match page.get(b"Contents") {
            Ok(lopdf::Object::Array(a)) => a.len(),
            Ok(_) => 1,
            Err(_) => 0,
        }
    };
    let mut doc = Document::from_bytes(sample_pdf(1, "flux"), None).unwrap();
    let before = count_streams(&doc.to_bytes().unwrap());
    let strokes: Vec<Vec<[f32; 2]>> = (0..5)
        .map(|i| {
            vec![
                [100.0 + 10.0 * i as f32, 100.0],
                [120.0 + 10.0 * i as f32, 130.0],
            ]
        })
        .collect();
    doc.insert_signature(
        0,
        rect(90.0, 90.0, 100.0, 50.0),
        &SignatureSource::Strokes { strokes },
    )
    .unwrap();
    // PDFium regroupe le contenu régénéré : au plus un flux de plus qu'avant, jamais un par trait.
    let after = count_streams(&doc.to_bytes().unwrap());
    assert!(after <= before + 1, "{before} → {after}");
}

/// Zone ou points hors de la page : refusés.
#[test]
fn signature_outside_page_is_refused() {
    let mut doc = Document::from_bytes(sample_pdf(1, "hors"), None).unwrap();
    assert!(matches!(
        doc.insert_signature(
            0,
            rect(5000.0, 0.0, 50.0, 50.0),
            &SignatureSource::Strokes {
                strokes: vec![vec![[5000.0, 0.0], [5010.0, 10.0]]]
            }
        ),
        Err(CoreError::InvalidOperation(_))
    ));
    assert!(matches!(
        doc.insert_signature(
            0,
            rect(10.0, 10.0, 50.0, 50.0),
            &SignatureSource::Strokes {
                strokes: vec![vec![[10.0, 10.0], [1e30, 10.0]]]
            }
        ),
        Err(CoreError::InvalidOperation(_))
    ));
}
