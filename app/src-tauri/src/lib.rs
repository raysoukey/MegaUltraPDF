// SPDX-License-Identifier: GPL-3.0-or-later
//! Backend Tauri de MegaUltraPDF.
//!
//! Fine couche entre le frontend (Svelte) et `mega-core` : des commandes `invoke`
//! et un protocole `mega://` qui sert les pages rendues. Aucune logique PDF ici.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod commands;
mod error;
mod protocol;
mod registry;

use std::sync::Mutex;

use registry::DocumentRegistry;

/// Point d'entrée de l'application (appelé par `main.rs`).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .manage(Mutex::new(DocumentRegistry::default()))
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::open_document,
            commands::close_document,
            commands::startup_file
        ]);
    protocol::register(builder)
        .run(tauri::generate_context!())
        .expect("erreur au lancement de l'application Tauri");
}
