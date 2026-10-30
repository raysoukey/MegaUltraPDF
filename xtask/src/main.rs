// SPDX-License-Identifier: GPL-3.0-or-later
//! Tâches de développement, lancées avec `cargo xtask <commande>`.
//!
//! Le motif « xtask » est une convention de l'écosystème Rust : plutôt qu'un
//! Makefile ou des scripts shell, les tâches de dev sont un petit binaire Rust
//! du workspace, donc portables et testables.

#![forbid(unsafe_code)]

mod brand;
mod licenses;
mod pdfium;
mod stage;

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};

const USAGE: &str = "\
Usage : cargo xtask <commande>

Commandes :
  fetch-pdfium [--target <triple>]   Télécharge la bibliothèque PDFium épinglée dans vendor/pdfium/
  gen-fixtures                       Régénère les PDF d'exemple de fixtures/ (sauf encrypted.pdf)
  stage-pdfium [--target <triple>]   Copie PDFium et sa licence dans app/src-tauri/resources/pdfium/
  licenses                           Inventaire des licences tierces (licenses.json, THIRD_PARTY_LICENSES.md)
  brand                              Icônes d'actions, logo et bannière à partir de assets/brand/
";

fn main() -> anyhow::Result<()> {
    // [Apprentissage] `std::env::args()` est un itérateur ; `skip(1)` saute le nom du
    // programme et `collect()` range le reste dans un `Vec<String>`.
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("fetch-pdfium") => fetch_pdfium(&args[1..]),
        Some("gen-fixtures") => gen_fixtures(),
        Some("stage-pdfium") => stage_pdfium(&args[1..]),
        Some("licenses") => licenses::generate(&workspace_root()),
        Some("brand") => brand::generate(&workspace_root()),
        Some(other) => bail!("commande inconnue : {other}\n\n{USAGE}"),
        None => {
            print!("{USAGE}");
            Ok(())
        }
    }
}

/// Racine du workspace : `xtask/` est un enfant direct de la racine.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/ a toujours un parent")
        .to_path_buf()
}

/// Triple de la machine hôte, lu dans la sortie de `rustc -vV` (ligne `host:`).
fn host_target() -> anyhow::Result<String> {
    let out = std::process::Command::new("rustc")
        .arg("-vV")
        .output()
        .context("impossible de lancer rustc")?;
    let text = String::from_utf8(out.stdout)?;
    text.lines()
        .find_map(|l| l.strip_prefix("host: "))
        .map(str::to_string)
        .context("ligne `host:` absente de la sortie de rustc -vV")
}

/// Cible demandée par `--target <triple>`, ou celle de la machine hôte.
fn target_arg(args: &[String], command: &str) -> anyhow::Result<String> {
    match args {
        [] => host_target(),
        [flag, value] if flag == "--target" => Ok(value.clone()),
        _ => bail!("arguments invalides pour {command}\n\n{USAGE}"),
    }
}

fn fetch_pdfium(args: &[String]) -> anyhow::Result<()> {
    pdfium::fetch(&workspace_root(), &target_arg(args, "fetch-pdfium")?)?;
    Ok(())
}

fn stage_pdfium(args: &[String]) -> anyhow::Result<()> {
    stage::stage(&workspace_root(), &target_arg(args, "stage-pdfium")?)?;
    Ok(())
}

/// Écrit `fixtures/simple.pdf` : 3 pages A4 titrées, générées par `mega-core`.
///
/// `fixtures/encrypted.pdf` et `fixtures/owner-only.pdf` ne sont pas régénérés ici : PDFium
/// ne sait pas chiffrer. Ils ont été produits une fois avec `qpdf --encrypt --user-password=secret
/// --owner-password=secret --bits=256 -- simple.pdf encrypted.pdf` et, pour la protection par
/// mot de passe propriétaire seul (s'ouvre sans mot de passe), `qpdf --encrypt "" secret 256 --
/// simple.pdf owner-only.pdf`.
fn gen_fixtures() -> anyhow::Result<()> {
    let dir = workspace_root().join("fixtures");
    std::fs::create_dir_all(&dir)?;
    let simple = dir.join("simple.pdf");
    std::fs::write(
        &simple,
        mega_core::testing::sample_pdf(3, "MegaUltraPDF — exemple"),
    )?;
    println!("écrit : {}", simple.display());
    let outlined = dir.join("outlined.pdf");
    std::fs::write(&outlined, mega_core::testing::outlined_pdf())?;
    println!("écrit : {}", outlined.display());
    let annotated = dir.join("annotated.pdf");
    std::fs::write(&annotated, mega_core::testing::annotated_pdf())?;
    println!("écrit : {}", annotated.display());
    let form = dir.join("form.pdf");
    std::fs::write(&form, mega_core::testing::form_pdf())?;
    println!("écrit : {}", form.display());
    let xfa = dir.join("xfa.pdf");
    std::fs::write(&xfa, mega_core::testing::xfa_pdf())?;
    println!("écrit : {}", xfa.display());
    // Page numérisée (image seule, aucun texte) pour l'OCR (Lot 8).
    let scanned = dir.join("scanned.pdf");
    std::fs::write(&scanned, mega_core::testing::scanned_pdf())?;
    println!("écrit : {}", scanned.display());
    let rotated = dir.join("rotated.pdf");
    std::fs::write(&rotated, mega_core::testing::rotated_pdf())?;
    println!("écrit : {}", rotated.display());
    // Texte dessiné dans le désordre : la sélection doit suivre l'ordre de lecture (0.12.2).
    let out_of_order = dir.join("out-of-order.pdf");
    std::fs::write(&out_of_order, mega_core::testing::out_of_order_pdf())?;
    println!("écrit : {}", out_of_order.display());
    let attachments = dir.join("attachments.pdf");
    std::fs::write(&attachments, mega_core::testing::attachments_pdf())?;
    println!("écrit : {}", attachments.display());
    // Caviardage d'une partie d'image (Lot 10).
    let mosaic = dir.join("image-mosaic.pdf");
    std::fs::write(&mosaic, mega_core::testing::image_mosaic_pdf())?;
    println!("écrit : {}", mosaic.display());
    Ok(())
}
