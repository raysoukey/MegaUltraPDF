// SPDX-License-Identifier: GPL-3.0-or-later
//! Backend Tauri de MegaUltraPDF.
//!
//! Fine couche entre le frontend (Svelte) et `mega-core` : des commandes `invoke`
//! asynchrones et un protocole `mega://` qui sert les pages rendues, tous deux adossés à
//! l'acteur PDFium ([`mega_core::service`]). Aucune logique PDF ici.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
// Politique d'erreurs (ADR-0005) : pas de panique dans le code de production.
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]

mod commands;
mod error;
mod protocol;

use std::path::{Path, PathBuf};

use mega_core::service::PdfService;

/// Dossier `pdfium/` des ressources de l'application installée, s'il existe.
///
/// Les paquets (NSIS, MSI, deb, AppImage, dmg) y embarquent la bibliothèque PDFium
/// (`cargo xtask stage-pdfium`, `tauri.release.conf.json`). En développement il n'existe
/// pas : `None`, et PDFium est cherché comme d'habitude (`MEGA_PDFIUM_DIR`, dossier de
/// l'exécutable, `vendor/`).
fn pdfium_resource_dir(resources: &Path) -> Option<PathBuf> {
    Some(resources.join("pdfium")).filter(|dir| dir.is_dir())
}

/// Point d'entrée de l'application (appelé par `main.rs`).
///
/// Si Tauri ne peut pas démarrer (WebView2 absent, configuration invalide), il n'y a rien
/// d'autre à faire que s'arrêter avec le message : `expect` est ici la bonne extrémité
/// de la chaîne d'erreurs (ADR-0005).
#[allow(clippy::expect_used)]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let context = tauri::generate_context!();
    // Avant le démarrage de l'acteur PDFium : dire au cœur où est la bibliothèque livrée
    // avec le paquet (sans effet si PDFium était déjà chargé, ce qui ne peut pas arriver ici).
    if let Ok(resources) =
        tauri::utils::platform::resource_dir(context.package_info(), &tauri::Env::default())
        && let Some(dir) = pdfium_resource_dir(&resources)
    {
        mega_core::engine::set_library_dir(dir);
    }
    let builder = tauri::Builder::default()
        // Un seul acteur PDFium pour toute l'application ; le handle est clonable.
        .manage(PdfService::spawn())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::open_document,
            commands::close_document,
            commands::set_generation,
            commands::text_layer,
            commands::search_document,
            commands::outline,
            commands::annotations,
            commands::page_objects,
            commands::form_fields,
            commands::form_info,
            commands::page_text,
            commands::export_images,
            commands::export_text,
            commands::security,
            commands::optimize_preview,
            commands::attachments,
            commands::save_attachment,
            commands::compare_pages,
            commands::ocr_status,
            commands::all_annotations,
            commands::all_form_fields,
            commands::set_tesseract_path,
            commands::pages_with_text,
            commands::signatures,
            commands::sign_document,
            commands::page_links,
            commands::document_properties,
            commands::file_size,
            commands::startup_file,
            commands::apply_edit,
            commands::undo_edit,
            commands::redo_edit,
            commands::edit_state,
            commands::save_document,
            commands::extract_pages,
            commands::merge_files,
            commands::split_document,
            commands::split_plan
        ]);
    protocol::register(builder)
        .run(context)
        .expect("erreur au lancement de l'application Tauri");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdfium_resource_dir_requires_the_staged_folder() {
        let resources = std::env::temp_dir().join(format!("mega-res-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&resources);
        std::fs::create_dir_all(&resources).unwrap();
        // Développement : aucun dossier `pdfium/` dans les ressources, on garde la recherche
        // habituelle (vendor/, MEGA_PDFIUM_DIR…).
        assert_eq!(pdfium_resource_dir(&resources), None);
        std::fs::create_dir_all(resources.join("pdfium")).unwrap();
        assert_eq!(
            pdfium_resource_dir(&resources),
            Some(resources.join("pdfium"))
        );
        std::fs::remove_dir_all(&resources).unwrap();
    }
}
