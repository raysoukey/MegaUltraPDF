// SPDX-License-Identifier: GPL-3.0-or-later
//! `mega extract` : copie des pages dans un nouveau fichier.

use std::path::Path;

use anyhow::Context;
use mega_core::{Document, PageSpec, edit::write_atomically};

/// Extrait les pages `spec` de `file` vers `out`.
pub fn run(file: &Path, spec: &str, out: &Path, password: Option<&str>) -> anyhow::Result<()> {
    let doc = Document::open(file, password)?;
    let pages = PageSpec::parse(spec, doc.page_count())?;
    let bytes = doc.extract_pages(&pages)?;
    write_atomically(out, &bytes).with_context(|| format!("écriture de {}", out.display()))?;
    println!("{} page(s) extraite(s) : {}", pages.len(), out.display());
    Ok(())
}
