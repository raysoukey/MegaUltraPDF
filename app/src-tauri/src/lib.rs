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

use mega_core::service::PdfService;

/// Point d'entrée de l'application (appelé par `main.rs`).
///
/// Si Tauri ne peut pas démarrer (WebView2 absent, configuration invalide), il n'y a rien
/// d'autre à faire que s'arrêter avec le message : `expect` est ici la bonne extrémité
/// de la chaîne d'erreurs (ADR-0005).
#[allow(clippy::expect_used)]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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
            commands::attachments,
            commands::save_attachment,
            commands::compare_pages,
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
        .run(tauri::generate_context!())
        .expect("erreur au lancement de l'application Tauri");
}
