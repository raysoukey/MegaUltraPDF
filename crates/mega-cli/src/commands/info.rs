// SPDX-License-Identifier: GPL-3.0-or-later
//! `mega info` : résumé d'un document.

use std::path::Path;

use mega_core::Document;

/// Affiche fichier, titre, nombre de pages et dimensions de la première page.
pub fn run(file: &Path, password: Option<&str>) -> anyhow::Result<()> {
    // `?` convertit automatiquement `CoreError` en `anyhow::Error` (grâce à `From`).
    let doc = Document::open(file, password)?;
    println!("Fichier : {}", file.display());
    println!(
        "Titre : {}",
        doc.title()
            .map(|t| display_safe(&t))
            .unwrap_or_else(|| "(aucun)".to_string())
    );
    println!("Pages : {}", doc.page_count());
    if doc.page_count() > 0 {
        let s = doc.page_size(0)?;
        println!("Page 1 : {:.1} × {:.1} pt", s.width_pt, s.height_pt);
    }
    Ok(())
}

/// Remplace les caractères de contrôle par « � » avant affichage dans un terminal.
///
/// Les métadonnées d'un PDF sont des données non fiables : un fichier malveillant
/// pourrait y glisser des séquences d'échappement ANSI qui manipulent le terminal.
fn display_safe(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { '\u{FFFD}' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_safe_neutralises_control_characters() {
        assert_eq!(
            display_safe("Titre \x1b[31mrouge\x07"),
            "Titre \u{FFFD}[31mrouge\u{FFFD}"
        );
        assert_eq!(display_safe("Résumé — été"), "Résumé — été");
    }
}
