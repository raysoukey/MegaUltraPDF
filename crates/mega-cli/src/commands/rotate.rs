// SPDX-License-Identifier: GPL-3.0-or-later
//! `mega rotate` : pivote des pages (rotation enregistrée) et écrit un nouveau fichier.

use std::path::Path;

use anyhow::Context;
use mega_core::{Document, PageSpec};

/// Pivote les pages `spec` de `degrees` (90, 180 ou 270, sens horaire) et écrit dans `out`.
pub fn run(
    file: &Path,
    spec: &str,
    degrees: u16,
    out: &Path,
    password: Option<&str>,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        matches!(degrees, 90 | 180 | 270),
        "l'angle doit être 90, 180 ou 270 degrés"
    );
    let mut doc = Document::open(file, password)?;
    let pages = PageSpec::parse(spec, doc.page_count())?;
    doc.rotate_pages(&pages, i32::from(degrees / 90))?;
    // `save_as` : écriture atomique **et** passe de nettoyage, comme dans l'application.
    doc.save_as(out)
        .with_context(|| format!("écriture de {}", out.display()))?;
    println!(
        "{} page(s) pivotée(s) de {degrees}° : {}",
        pages.len(),
        out.display()
    );
    Ok(())
}
