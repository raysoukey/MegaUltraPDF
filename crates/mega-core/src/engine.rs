// SPDX-License-Identifier: GPL-3.0-or-later
//! Localisation et initialisation de la bibliothèque PDFium.
//!
//! PDFium est chargée **une seule fois** par processus et partagée par tous les
//! documents. La crate `pdfium-render` (feature `thread_safe`, activée par défaut)
//! sérialise les appels derrière un verrou, car PDFium elle-même n'est pas conçue
//! pour être appelée depuis plusieurs threads à la fois.

use std::path::PathBuf;
use std::sync::OnceLock;

use pdfium_render::prelude::Pdfium;

use crate::error::{CoreError, CoreResult};

/// Variable d'environnement permettant d'imposer le dossier contenant PDFium.
pub const LIBRARY_DIR_ENV: &str = "MEGA_PDFIUM_DIR";

/// Dossiers où chercher la bibliothèque PDFium, dans l'ordre.
///
/// 1. le dossier donné par `MEGA_PDFIUM_DIR` ;
/// 2. le dossier de l'exécutable (cas d'une application installée) ;
/// 3. `vendor/pdfium/<triple>/` à la racine du workspace (cas de `cargo run` et
///    `cargo test`, rempli par `cargo xtask fetch-pdfium`).
pub fn library_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(dir) = std::env::var_os(LIBRARY_DIR_ENV) {
        paths.push(PathBuf::from(dir));
    }
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
    {
        paths.push(exe_dir);
    }
    // `CARGO_MANIFEST_DIR` vaut `<racine>/crates/mega-core` ; on remonte de deux niveaux.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    paths.push(
        workspace_root
            .join("vendor")
            .join("pdfium")
            .join(env!("MEGA_TARGET_TRIPLE")),
    );
    paths
}

/// Retourne l'instance PDFium partagée, en la créant au premier appel.
///
/// \[Apprentissage\] `OnceLock<T>` est une case « écrite une seule fois ». `get()` la lit,
/// `set()` l'écrit si elle est vide. Deux threads qui arrivent en même temps peuvent
/// tous deux tenter le chargement ; le second `set` échoue et son instance est
/// simplement abandonnée. C'est acceptable ici : charger la DLL deux fois est rare et
/// sans conséquence, et cela évite un verrou explicite.
pub(crate) fn pdfium() -> CoreResult<&'static Pdfium> {
    static PDFIUM: OnceLock<Pdfium> = OnceLock::new();
    if let Some(p) = PDFIUM.get() {
        return Ok(p);
    }
    let fresh = load()?;
    let _ = PDFIUM.set(fresh);
    Ok(PDFIUM.get().expect("vient d'être initialisé"))
}

fn load() -> CoreResult<Pdfium> {
    let searched = library_search_paths();
    for dir in &searched {
        let candidate = Pdfium::pdfium_platform_library_name_at_path(dir);
        if let Ok(bindings) = Pdfium::bind_to_library(&candidate) {
            return Ok(Pdfium::new(bindings));
        }
    }
    // Dernier recours : une PDFium installée au niveau du système.
    match Pdfium::bind_to_system_library() {
        Ok(bindings) => Ok(Pdfium::new(bindings)),
        Err(_) => Err(CoreError::LibraryNotFound { searched }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_paths_end_with_vendor_dir_for_this_target() {
        let last = library_search_paths().pop().unwrap();
        assert!(last.ends_with(env!("MEGA_TARGET_TRIPLE")));
        assert!(last.to_string_lossy().contains("vendor"));
    }
}
