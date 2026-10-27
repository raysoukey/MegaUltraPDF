// SPDX-License-Identifier: GPL-3.0-or-later
//! `mega flatten` : aplatit formulaire et annotations dans un nouveau fichier.

use std::path::Path;

use anyhow::Context;
use mega_core::{Document, edit::write_atomically};

/// Aplatit toutes les pages de `file` et écrit le résultat dans `out`.
pub fn run(file: &Path, out: &Path, password: Option<&str>) -> anyhow::Result<()> {
    let mut doc = Document::open(file, password)?;
    doc.flatten_all()?;
    write_atomically(out, &doc.to_bytes()?)
        .with_context(|| format!("écriture de {}", out.display()))?;
    println!("Formulaire aplati : {}", out.display());
    Ok(())
}
