// SPDX-License-Identifier: GPL-3.0-or-later
//! `mega split` : scinde un PDF en plusieurs fichiers.

use std::path::Path;

use anyhow::Context;
use mega_core::{Document, PageSpec, edit::write_atomically, pages};

/// Comment découper.
pub enum SplitMode<'a> {
    /// Plages explicites (« 1-2,3-5 »), une partie par plage.
    Ranges(&'a str),
    /// Toutes les `n` pages.
    Every(u16),
    /// Aux signets de premier niveau.
    Bookmarks,
}

/// Écrit `<prefix>-N.pdf` dans le dossier `out_dir` et affiche les fichiers créés.
pub fn run(
    file: &Path,
    mode: SplitMode<'_>,
    out_dir: &Path,
    prefix: Option<&str>,
    password: Option<&str>,
) -> anyhow::Result<()> {
    let doc = Document::open(file, password)?;
    let count = doc.page_count();
    let parts = match mode {
        SplitMode::Ranges(spec) => PageSpec::ranges(spec, count)?,
        SplitMode::Every(n) => PageSpec::every(n, count),
        SplitMode::Bookmarks => {
            let starts: Vec<u16> = doc.outline()?.iter().filter_map(|n| n.page).collect();
            anyhow::ensure!(!starts.is_empty(), "ce document n'a pas de signets");
            PageSpec::split_at(&starts, count)
        }
    };
    let prefix = prefix
        .map(str::to_string)
        .or_else(|| file.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "partie".to_string());
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("création du dossier {}", out_dir.display()))?;
    let files = pages::split(&doc, &parts)?;
    for (i, bytes) in files.iter().enumerate() {
        let path = out_dir.join(format!("{prefix}-{}.pdf", i + 1));
        write_atomically(&path, bytes)
            .with_context(|| format!("écriture de {}", path.display()))?;
        println!("{} ({} pages)", path.display(), parts[i].len());
    }
    Ok(())
}
