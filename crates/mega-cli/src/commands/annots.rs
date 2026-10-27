// SPDX-License-Identifier: GPL-3.0-or-later
//! `mega annots` : liste les annotations d'un PDF (banc d'essai du modèle de lecture).

use std::path::Path;

use mega_core::{Annotation, AnnotationKind, Document};

/// Nom court et stable d'un genre (identique au `kind` sérialisé pour le frontend).
fn kind_name(kind: AnnotationKind) -> &'static str {
    match kind {
        AnnotationKind::Note => "note",
        AnnotationKind::Highlight => "highlight",
        AnnotationKind::Underline => "underline",
        AnnotationKind::StrikeOut => "strike_out",
        AnnotationKind::Squiggly => "squiggly",
        AnnotationKind::Rect => "rect",
        AnnotationKind::Ellipse => "ellipse",
        AnnotationKind::Line => "line",
        AnnotationKind::Arrow => "arrow",
        AnnotationKind::Ink => "ink",
        AnnotationKind::Text => "text",
        AnnotationKind::Stamp => "stamp",
        AnnotationKind::Link => "link",
        AnnotationKind::Widget => "widget",
        AnnotationKind::Other => "other",
    }
}

/// Une ligne par annotation : `p<page> #<index> <genre> [x y w h] auteur="…" "<contenu>"`.
fn line(page: u16, a: &Annotation) -> String {
    let contents: String = a
        .contents
        .as_deref()
        .unwrap_or("")
        .chars()
        .take(40)
        .collect();
    format!(
        "p{} #{} {} [{:.0} {:.0} {:.0} {:.0}] auteur=\"{}\" \"{}\"",
        page + 1,
        a.index,
        kind_name(a.kind),
        a.rect.x,
        a.rect.y,
        a.rect.w,
        a.rect.h,
        super::info::display_safe(a.author.as_deref().unwrap_or("")),
        super::info::display_safe(&contents)
    )
}

/// Liste les annotations de `page` (1-based) ou de toutes les pages.
pub fn run(file: &Path, page: Option<u16>, password: Option<&str>) -> anyhow::Result<()> {
    let doc = Document::open(file, password)?;
    let pages: Vec<u16> = match page {
        Some(p) => {
            anyhow::ensure!(
                p >= 1 && p <= doc.page_count(),
                "la page doit être comprise entre 1 et {}",
                doc.page_count()
            );
            vec![p - 1]
        }
        None => (0..doc.page_count()).collect(),
    };
    let mut total = 0usize;
    for p in pages {
        for a in doc.annotations(p)? {
            println!("{}", line(p, &a));
            total += 1;
        }
    }
    println!("{total} annotation{}", if total > 1 { "s" } else { "" });
    Ok(())
}
