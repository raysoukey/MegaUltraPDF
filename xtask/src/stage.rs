// SPDX-License-Identifier: GPL-3.0-or-later
//! `cargo xtask stage-pdfium` : prépare PDFium pour le bundler Tauri.
//!
//! Copie la bibliothèque PDFium de la cible et sa notice de licence (téléchargées par
//! `fetch-pdfium` dans `vendor/pdfium/<triple>/`) vers `app/src-tauri/resources/pdfium/`,
//! dossier ignoré par git que `tauri.release.conf.json` embarque dans les paquets. Au
//! démarrage, l'application installée y cherche PDFium en premier
//! (`mega_core::engine::set_library_dir`).

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};

use crate::pdfium;

/// Nom du fichier de bibliothèque PDFium pour une cible (`None` si la cible n'a pas
/// d'archive épinglée dans `pdfium.lock.toml`).
pub fn staged_library_name(target: &str) -> Option<&'static str> {
    pdfium::asset_for_target(target).map(pdfium::library_file_name)
}

/// Copie la bibliothèque et sa licence dans le dossier de ressources ; retourne ce dossier.
pub fn stage(workspace_root: &Path, target: &str) -> anyhow::Result<PathBuf> {
    let name = staged_library_name(target)
        .with_context(|| format!("aucune archive PDFium connue pour la cible {target}"))?;
    let vendor = workspace_root.join("vendor").join("pdfium").join(target);
    let library = vendor.join(name);
    let notice = vendor.join(pdfium::NOTICE_FILE);
    if !library.exists() || !notice.exists() {
        bail!(
            "PDFium absent de {} : lancez d'abord `cargo xtask fetch-pdfium --target {target}`",
            vendor.display()
        );
    }
    let dest = workspace_root
        .join("app")
        .join("src-tauri")
        .join("resources")
        .join("pdfium");
    // On repart d'un dossier vide : pas de bibliothèque d'une autre cible qui traîne.
    if dest.exists() {
        std::fs::remove_dir_all(&dest)
            .with_context(|| format!("nettoyage de {}", dest.display()))?;
    }
    std::fs::create_dir_all(&dest).with_context(|| format!("création de {}", dest.display()))?;
    std::fs::copy(&library, dest.join(name))
        .with_context(|| format!("copie de {}", library.display()))?;
    std::fs::copy(&notice, dest.join(pdfium::NOTICE_FILE))
        .with_context(|| format!("copie de {}", notice.display()))?;
    println!("PDFium prêt pour le bundler : {}", dest.display());
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staged_library_name_per_target() {
        assert_eq!(
            staged_library_name("x86_64-pc-windows-msvc"),
            Some("pdfium.dll")
        );
        assert_eq!(
            staged_library_name("x86_64-unknown-linux-gnu"),
            Some("libpdfium.so")
        );
        assert_eq!(
            staged_library_name("aarch64-apple-darwin"),
            Some("libpdfium.dylib")
        );
        assert_eq!(staged_library_name("riscv64gc-unknown-linux-gnu"), None);
    }
}
