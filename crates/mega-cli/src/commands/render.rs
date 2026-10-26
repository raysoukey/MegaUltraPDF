// SPDX-License-Identifier: GPL-3.0-or-later
//! `mega render` : rend une page en PNG.

use std::path::Path;

use anyhow::Context;
use mega_core::Document;

/// Rend la page `page` (numérotée à partir de 1) à l'échelle `scale` dans `out`.
pub fn run(
    file: &Path,
    page: u16,
    scale: f32,
    out: &Path,
    password: Option<&str>,
) -> anyhow::Result<()> {
    anyhow::ensure!(page >= 1, "le numéro de page commence à 1");
    let doc = Document::open(file, password)?;
    let rendered = doc.render_page(page - 1, scale)?;
    std::fs::write(out, rendered.to_png()?)
        .with_context(|| format!("écriture de {}", out.display()))?;
    println!(
        "Page {page} rendue en {}×{} px : {}",
        rendered.width_px,
        rendered.height_px,
        out.display()
    );
    Ok(())
}
