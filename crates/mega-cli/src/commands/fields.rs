// SPDX-License-Identifier: GPL-3.0-or-later
//! `mega fields` : liste les champs de formulaire d'un PDF.

use std::path::Path;

use mega_core::{Document, FieldKind, FormField, FormType};

fn kind_name(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::Text => "text",
        FieldKind::Checkbox => "checkbox",
        FieldKind::Radio => "radio",
        FieldKind::Combo => "combo",
        FieldKind::List => "list",
        FieldKind::Button => "button",
        FieldKind::Signature => "signature",
        FieldKind::Unknown => "unknown",
    }
}

fn value_of(f: &FormField) -> String {
    match f.checked {
        Some(true) => "oui".to_string(),
        Some(false) => "non".to_string(),
        None => f
            .value
            .as_deref()
            .map(super::info::display_safe)
            .unwrap_or_else(|| "-".to_string()),
    }
}

/// Une ligne par champ : `p<page> <nom> [<genre>] = <valeur>` (+ `(lecture seule)`, `(requis)`).
pub fn run(file: &Path, password: Option<&str>) -> anyhow::Result<()> {
    let doc = Document::open(file, password)?;
    let info = doc.form_info()?;
    if info.form_type == FormType::Xfa {
        println!("Formulaire XFA : non pris en charge (affichage seul).");
    }
    let mut total = 0usize;
    for page in 0..doc.page_count() {
        for f in doc.form_fields(page)? {
            let mut flags = String::new();
            if f.read_only {
                flags.push_str(" (lecture seule)");
            }
            if f.required {
                flags.push_str(" (requis)");
            }
            println!(
                "p{} {} [{}] = {}{flags}",
                page + 1,
                super::info::display_safe(&f.name),
                kind_name(f.kind),
                value_of(&f)
            );
            total += 1;
        }
    }
    println!("{total} champ{}", if total > 1 { "s" } else { "" });
    Ok(())
}
