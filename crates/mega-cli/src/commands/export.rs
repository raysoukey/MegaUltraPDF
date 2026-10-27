// SPDX-License-Identifier: GPL-3.0-or-later
//! `mega export-images` et `mega export-text` : sortir des pages en images ou en texte.

use std::path::Path;

use anyhow::Context;
use mega_core::edit::write_atomically;
use mega_core::export::{ImageExport, ImageFormat};
use mega_core::{Document, PageSpec};

/// Pages visées : toutes si `spec` est absent.
pub(crate) fn pages_of(doc: &Document, spec: Option<&str>) -> anyhow::Result<Option<Vec<u16>>> {
    spec.map(|s| PageSpec::parse(s, doc.page_count()).map_err(anyhow::Error::from))
        .transpose()
}

/// Exporte des pages en PNG ou JPEG dans `dir`.
#[allow(clippy::too_many_arguments)]
pub fn images(
    file: &Path,
    dir: &Path,
    pages: Option<&str>,
    dpi: u16,
    jpeg: bool,
    quality: u8,
    prefix: Option<&str>,
    force: bool,
    password: Option<&str>,
) -> anyhow::Result<()> {
    let doc = Document::open(file, password)?;
    let pages = pages_of(&doc, pages)?;
    let prefix = prefix.map(str::to_string).unwrap_or_else(|| {
        file.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "page".to_string())
    });
    let spec = ImageExport {
        pages,
        format: if jpeg {
            ImageFormat::Jpeg { quality }
        } else {
            ImageFormat::Png
        },
        dpi,
        with_annotations: true,
    };
    let written = doc.export_images(&spec, dir, &prefix, force)?;
    println!(
        "{} image{} écrite{} dans {}",
        written.len(),
        if written.len() > 1 { "s" } else { "" },
        if written.len() > 1 { "s" } else { "" },
        dir.display()
    );
    Ok(())
}

/// Exporte le texte des pages dans un fichier UTF-8.
pub fn text(
    file: &Path,
    out: &Path,
    pages: Option<&str>,
    separators: bool,
    password: Option<&str>,
) -> anyhow::Result<()> {
    let doc = Document::open(file, password)?;
    let pages = pages_of(&doc, pages)?;
    let text = doc.export_text(pages.as_deref(), separators)?;
    write_atomically(out, text.as_bytes())
        .with_context(|| format!("écriture de {}", out.display()))?;
    println!("Texte écrit : {}", out.display());
    Ok(())
}
