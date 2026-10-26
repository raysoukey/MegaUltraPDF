// SPDX-License-Identifier: GPL-3.0-or-later
//! `mega merge` : concatène plusieurs PDF dans un nouveau fichier.

use std::path::Path;

use anyhow::Context;
use mega_core::{Document, edit::write_atomically, pages};

/// Fusionne `files` dans l'ordre et écrit le résultat dans `out` (écriture atomique).
pub fn run(files: &[impl AsRef<Path>], out: &Path) -> anyhow::Result<()> {
    let docs = files
        .iter()
        .map(|f| {
            Document::open(f.as_ref(), None)
                .with_context(|| format!("ouverture de {}", f.as_ref().display()))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let refs: Vec<&Document> = docs.iter().collect();
    let bytes = pages::merge(&refs)?;
    write_atomically(out, &bytes).with_context(|| format!("écriture de {}", out.display()))?;
    let total: u32 = docs.iter().map(|d| u32::from(d.page_count())).sum();
    println!(
        "{} fichiers fusionnés ({total} pages) : {}",
        docs.len(),
        out.display()
    );
    Ok(())
}
