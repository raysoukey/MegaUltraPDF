// SPDX-License-Identifier: GPL-3.0-or-later
//! `mega optimize`, `mega attachments`, `mega compare`.

use std::path::Path;

use anyhow::Context;
use mega_core::Document;
use mega_core::compare::compare_pages;
use mega_core::edit::write_atomically;
use mega_core::optimize::OptimizeOptions;

/// Optimise `file` et écrit `out`, avec le rapport.
pub fn optimize(
    file: &Path,
    quality: Option<u8>,
    max_px: Option<u32>,
    compress: bool,
    out: &Path,
    password: Option<&str>,
) -> anyhow::Result<()> {
    let mut doc = Document::open(file, password)?;
    let options = OptimizeOptions {
        image_quality: quality,
        max_image_px: max_px,
        compress_streams: compress,
    };
    // L'appel direct donne le rapport ; l'historique n'en a pas besoin en ligne de commande.
    let report = doc.optimize(&options)?;
    doc.save_as(out)
        .with_context(|| format!("écriture de {}", out.display()))?;
    println!(
        "{} → {} octets ({} image(s) recomprimée(s), {} objet(s) retiré(s)) : {}",
        report.bytes_before,
        report.bytes_after,
        report.images_recompressed,
        report.objects_removed,
        out.display()
    );
    Ok(())
}

/// Liste les pièces jointes ; avec `extract`, écrit la pièce `extract` dans `out`.
pub fn attachments(
    file: &Path,
    extract: Option<u16>,
    out: Option<&Path>,
    password: Option<&str>,
) -> anyhow::Result<()> {
    let doc = Document::open(file, password)?;
    if let Some(index) = extract {
        let out = out.ok_or_else(|| anyhow::anyhow!("--extract exige -o <fichier>"))?;
        doc.save_attachment(index, out)?;
        println!("Pièce jointe {index} écrite : {}", out.display());
        return Ok(());
    }
    let list = doc.attachments()?;
    for a in &list {
        println!(
            "#{} {} ({} o)",
            a.index,
            super::info::display_safe(&a.name),
            a.size
        );
    }
    println!(
        "{} pièce{} jointe{}",
        list.len(),
        if list.len() > 1 { "s" } else { "" },
        if list.len() > 1 { "s" } else { "" }
    );
    Ok(())
}

/// Compare une page de `a` et de `b` ; écrit l'image des différences en PNG dans `out`.
pub fn compare(
    a: &Path,
    b: &Path,
    page_a: u16,
    page_b: u16,
    scale: f32,
    out: &Path,
    password: Option<&str>,
) -> anyhow::Result<()> {
    let doc_a = Document::open(a, password)?;
    let doc_b = Document::open(b, password)?;
    anyhow::ensure!(
        page_a >= 1 && page_a <= doc_a.page_count(),
        "la page de {} doit être comprise entre 1 et {}",
        a.display(),
        doc_a.page_count()
    );
    anyhow::ensure!(
        page_b >= 1 && page_b <= doc_b.page_count(),
        "la page de {} doit être comprise entre 1 et {}",
        b.display(),
        doc_b.page_count()
    );
    let diff = compare_pages(&doc_a, page_a - 1, &doc_b, page_b - 1, scale)?;
    write_atomically(out, &diff.image.to_png()?)
        .with_context(|| format!("écriture de {}", out.display()))?;
    println!(
        "{} pixels différents sur {} ({:.1} %) : {}",
        diff.differing_pixels,
        diff.total_pixels,
        100.0 * diff.differing_pixels as f64 / diff.total_pixels.max(1) as f64,
        out.display()
    );
    Ok(())
}
