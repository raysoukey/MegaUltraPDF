// SPDX-License-Identifier: GPL-3.0-or-later
//! Localisation et initialisation de la bibliothèque PDFium.
//!
//! PDFium est chargée **une seule fois** par processus et partagée par tous les
//! documents. La crate `pdfium-render` (feature `thread_safe`, activée par défaut)
//! sérialise les appels derrière un verrou, car PDFium elle-même n'est pas conçue
//! pour être appelée depuis plusieurs threads à la fois.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use pdfium_render::prelude::{PdfDocument, Pdfium};

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
/// \[Apprentissage\] `OnceLock<T>` est une case « écrite une seule fois ».
/// `get_or_init(f)` garantit que `f` s'exécute **exactement une fois**, même si
/// plusieurs threads arrivent en même temps : les autres attendent, puis lisent la
/// valeur. C'est indispensable ici : une première version de cette fonction laissait
/// deux threads charger PDFium en parallèle (« le second résultat est jeté, pas
/// grave ») et le processus plantait sur macOS arm64 — PDFium ne tolère pas deux
/// initialisations simultanées. Le test `concurrent_first_use_is_safe` garde ce cas.
///
/// Le résultat d'un échec de chargement est lui aussi mémorisé : si la bibliothèque
/// manque, il faut la récupérer (`cargo xtask fetch-pdfium`) puis relancer le programme.
pub(crate) fn pdfium() -> CoreResult<&'static Pdfium> {
    static PDFIUM: OnceLock<Result<Pdfium, Vec<PathBuf>>> = OnceLock::new();
    match PDFIUM.get_or_init(load) {
        Ok(p) => Ok(p),
        Err(searched) => Err(CoreError::LibraryNotFound {
            searched: searched.clone(),
        }),
    }
}

/// Charge un document PDF depuis des octets, de façon atomique vis-à-vis des autres
/// chargements.
///
/// \[Apprentissage\] PDFium range le code d'erreur du dernier chargement dans une
/// **variable globale** (`FPDF_GetLastError`). `pdfium-render` fait deux appels :
/// charger, puis lire ce code si le chargement a échoué. La feature `thread_safe`
/// verrouille chaque appel séparément, donc un chargement réussi sur un autre thread
/// peut s'intercaler et effacer le code (« fichier invalide » devenait « erreur
/// inconnue », vu en CI Linux/macOS). Ce `Mutex` rend la paire d'appels indivisible.
/// `unwrap_or_else(|e| e.into_inner())` récupère le verrou même si un thread a paniqué
/// en le tenant : il ne protège aucune donnée, seulement l'ordre des appels.
pub(crate) fn load_bytes(
    bytes: Vec<u8>,
    password: Option<&str>,
) -> CoreResult<PdfDocument<'static>> {
    static LOAD_LOCK: Mutex<()> = Mutex::new(());
    let _guard = LOAD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    Ok(pdfium()?.load_pdf_from_byte_vec(bytes, password)?)
}

/// Charge la bibliothèque ; en cas d'échec, retourne la liste des dossiers essayés.
fn load() -> Result<Pdfium, Vec<PathBuf>> {
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
        Err(_) => Err(searched),
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
