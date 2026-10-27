// SPDX-License-Identifier: GPL-3.0-or-later
//! `mega ocr` (feature `ocr`, Lot 8) : rend cherchables les pages numérisées.

use std::path::{Path, PathBuf};

use anyhow::Context;
use mega_core::Document;
use mega_core::ocr;
use mega_core::pagespec::PageSpec;

/// Reconnaît le texte des pages visées qui n'en ont pas, puis écrit `out`.
pub fn run(
    file: &Path,
    out: &Path,
    languages: &str,
    pages: Option<&str>,
    tesseract: Option<PathBuf>,
    password: Option<&str>,
) -> anyhow::Result<()> {
    let exe = match tesseract {
        Some(path) => path,
        None => ocr::find_tesseract().context(
            "Tesseract introuvable : installez-le, ou indiquez son chemin avec --tesseract",
        )?,
    };
    // « eng+fra » ou « eng,fra » : plusieurs langues.
    let languages: Vec<String> = languages
        .split(['+', ','])
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    let mut doc = Document::open(file, password)?;
    let targets = match pages {
        Some(spec) => PageSpec::parse(spec, doc.page_count())?,
        None => (0..doc.page_count()).collect(),
    };
    let mut done = 0;
    for page in targets {
        if doc.page_has_text(page)? {
            println!(
                "Page {} : contient déjà du texte, ignorée",
                u32::from(page) + 1
            );
            continue;
        }
        doc.ocr_page_with(page, &languages, &exe)?;
        println!("Page {} : texte reconnu", u32::from(page) + 1);
        done += 1;
    }
    if done == 0 {
        println!("Aucune page à reconnaître : rien n'a été écrit.");
        return Ok(());
    }
    doc.save_as(out)
        .with_context(|| format!("écriture de {}", out.display()))?;
    println!("{done} page(s) reconnue(s) : {}", out.display());
    Ok(())
}
