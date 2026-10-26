// SPDX-License-Identifier: GPL-3.0-or-later
//! Backend Tauri de MegaUltraPDF.
//!
//! Fine couche entre le frontend (Svelte) et `mega-core` : des commandes `invoke`
//! et un protocole `mega://` qui sert les pages rendues. Aucune logique PDF ici.

#![forbid(unsafe_code)]

/// Point d'entrée de l'application (appelé par `main.rs`).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .run(tauri::generate_context!())
        .expect("erreur au lancement de l'application Tauri");
}
