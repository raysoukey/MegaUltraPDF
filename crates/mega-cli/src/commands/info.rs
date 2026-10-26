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
        .map(|c| {
            if is_display_dangerous(c) {
                '\u{FFFD}'
            } else {
                c
            }
        })
        .collect()
}

/// Caractères de contrôle, invisibles ou de réordonnancement bidirectionnel.
///
/// Ces caractères n'ont pas de rendu propre mais changent ce que l'utilisateur
/// croit lire (texte inversé, mots cachés). Liste explicite plutôt qu'une table
/// Unicode complète : pas de dépendance pour un cas aussi simple.
fn is_display_dangerous(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{00AD}'                 // trait d'union conditionnel
            | '\u{200B}'..='\u{200F}'  // espaces de largeur nulle, marques de direction
            | '\u{2028}' | '\u{2029}'  // séparateurs de ligne et de paragraphe
            | '\u{202A}'..='\u{202E}'  // incrustations et forçages bidirectionnels
            | '\u{2060}'..='\u{206F}'  // gluon de mots, isolats bidirectionnels, invisibles
            | '\u{FEFF}'               // BOM / espace insécable de largeur nulle
            | '\u{E000}'..='\u{F8FF}'  // zone à usage privé (glyphes imprévisibles)
        )
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
        // Caractères invisibles ou de réordonnancement bidirectionnel (usurpation visuelle).
        assert_eq!(display_safe("a\u{202E}b\u{200B}c"), "a\u{FFFD}b\u{FFFD}c");
    }
}
