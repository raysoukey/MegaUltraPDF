// SPDX-License-Identifier: GPL-3.0-or-later
//! Formulaires : lecture des champs, remplissage, aplatissement, signature visuelle.

use mega_core::testing::{form_pdf, sample_pdf, xfa_pdf};
use mega_core::{Document, FieldKind, FormType};

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
