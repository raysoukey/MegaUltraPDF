// SPDX-License-Identifier: GPL-3.0-or-later
//! `mega info` : résumé d'un document.

use std::path::Path;

use mega_core::Document;

/// Affiche fichier, titre, nombre de pages et dimensions de la première page.
pub fn run(file: &Path, password: Option<&str>) -> anyhow::Result<()> {
    // `?` convertit automatiquement `CoreError` en `anyhow::Error` (grâce à `From`).
    let doc = Document::open(file, password)?;
    println!("Fichier : {}", file.display());
    println!("Titre : {}", doc.title().as_deref().unwrap_or("(aucun)"));
    println!("Pages : {}", doc.page_count());
    if doc.page_count() > 0 {
        let s = doc.page_size(0)?;
        println!("Page 1 : {:.1} × {:.1} pt", s.width_pt, s.height_pt);
    }
    Ok(())
}
