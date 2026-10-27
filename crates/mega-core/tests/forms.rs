// SPDX-License-Identifier: GPL-3.0-or-later
//! Formulaires : lecture des champs, remplissage, aplatissement, signature visuelle.

use mega_core::testing::{form_pdf, form_pdf_without_need_appearances, sample_pdf, xfa_pdf};
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
    doc.set_field_value(0, 2, &FieldValue::Checked { checked: true })
        .unwrap();
    let fields = reopen(&doc).form_fields(0).unwrap();
    assert_eq!(fields[2].checked, Some(true));
    doc.set_field_value(0, 2, &FieldValue::Checked { checked: false })
        .unwrap();
    assert_eq!(doc.form_fields(0).unwrap()[2].checked, Some(false));
    // Radio : pdfium-render n'écrit que `/V` du bouton visé (pas `/AS` des autres) : le
    // groupe deviendrait incohérent. Refusé, et rien ne change (A reste coché).
    assert!(matches!(
        doc.set_field_value(0, 4, &FieldValue::Checked { checked: true }),
        Err(CoreError::InvalidOperation(_))
    ));
    let fields = reopen(&doc).form_fields(0).unwrap();
    assert_eq!(fields[3].checked, Some(true), "A toujours coché");
    assert_eq!(fields[4].checked, Some(false), "B toujours décoché");
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
        doc.set_field_value(0, 5, &text("Lyon")),
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
    // Sans formulaire : succès sans changement.
    let mut plain = Document::from_bytes(sample_pdf(1, "plat"), None).unwrap();
    plain.flatten_all().unwrap();
    assert_eq!(plain.page_count(), 1);
}
