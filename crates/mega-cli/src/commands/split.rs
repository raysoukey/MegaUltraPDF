// SPDX-License-Identifier: GPL-3.0-or-later
//! `mega split` : scinde un PDF en plusieurs fichiers.

use std::path::Path;

use mega_core::Document;
use mega_core::pages::{SplitMode, plan_split, split_to_dir};

/// Écrit `<prefix>-N.pdf` dans le dossier `out_dir` et affiche les fichiers créés.
/// Sans `force`, refuse d'écraser un fichier existant.
pub fn run(
    file: &Path,
    mode: SplitMode,
    out_dir: &Path,
    prefix: Option<&str>,
    force: bool,
    password: Option<&str>,
) -> anyhow::Result<()> {
    let doc = Document::open(file, password)?;
    let parts = plan_split(&doc, &mode)?;
    let prefix = prefix
        .map(str::to_string)
        .or_else(|| file.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "partie".to_string());
    let paths = split_to_dir(&doc, &parts, out_dir, &prefix, force)?;
    for (path, part) in paths.iter().zip(&parts) {
        println!("{} ({} pages)", path.display(), part.len());
    }
    Ok(())
}
