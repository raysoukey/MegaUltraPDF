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
mod default_app;
mod diagnostic;
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

/// Chemins des fichiers d'une liste d'URL : celles `file://` seulement (espaces et accents
/// décodés). Sert à l'ouverture depuis le Finder de macOS, qui donne des URL (C33).
// Utilisée par l'événement macOS seulement (et par son test sur les autres systèmes).
#[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(dead_code))]
fn file_paths(urls: &[tauri::Url]) -> Vec<PathBuf> {
    urls.iter()
        .filter(|u| u.scheme() == "file")
        .filter_map(|u| u.to_file_path().ok())
        .collect()
}

/// Fichiers à ouvrir reçus du système (Finder) avant que l'interface les écoute : au
/// démarrage à froid, l'événement arrive avant le chargement de la page, et un événement
/// Tauri non écouté est perdu. L'interface les réclame une fois prête (relecture du Lot 9).
#[derive(Default)]
pub struct PendingPaths(std::sync::Mutex<Vec<String>>);

impl PendingPaths {
    /// Ajoute des chemins en attente.
    pub fn push(&self, paths: Vec<String>) {
        if let Ok(mut list) = self.0.lock() {
            list.extend(paths);
        }
    }

    /// Rend les chemins en attente et vide la liste.
    pub fn take(&self) -> Vec<String> {
        self.0
            .lock()
            .map(|mut list| std::mem::take(&mut *list))
            .unwrap_or_default()
    }
}

/// Chemins PDF des arguments d'une **seconde instance** (lancée pendant que l'application
/// tourne) : le premier argument est l'exécutable, les chemins relatifs se lisent dans le
/// dossier courant de cette seconde instance.
fn second_instance_paths(argv: &[String], cwd: &str) -> Vec<String> {
    commands::pdf_paths_from_args(argv.iter().skip(1).cloned(), Path::new(cwd))
}

/// Relaie à la fenêtre existante les fichiers d'une seconde instance (Lot 11) : gardés dans
/// [`PendingPaths`] et signalés par `open-paths` (comme le Finder sous macOS), puis la
/// fenêtre revient au premier plan.
#[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
fn relay_second_instance(app: &tauri::AppHandle, argv: &[String], cwd: &str) {
    use tauri::{Emitter, Manager};
    let paths = second_instance_paths(argv, cwd);
    if !paths.is_empty() {
        app.state::<PendingPaths>().push(paths);
        let _ = app.emit("open-paths", ());
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
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
    let builder = tauri::Builder::default();
    // Enregistré en premier (exigence du greffon) : une seconde instance confie ses fichiers
    // à la première et s'arrête aussitôt. macOS garde de lui-même une seule instance.
    #[cfg(any(windows, target_os = "linux"))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
        relay_second_instance(app, &argv, &cwd);
    }));
    let builder = builder
        // Journal de diagnostic local (Lot 13) : dans le dossier de journaux de l'application.
        .setup(|app| {
            use tauri::Manager;
            if let Ok(dir) = app.path().app_log_dir() {
                diagnostic::init(dir);
            }
            Ok(())
        })
        // Un seul acteur PDFium pour toute l'application ; le handle est clonable.
        .manage(PdfService::spawn())
        .manage(PendingPaths::default())
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
            commands::take_pending_paths,
            commands::pages_with_text,
            commands::signatures,
            commands::sign_document,
            commands::page_links,
            commands::document_properties,
            commands::file_size,
            commands::startup_files,
            commands::default_app_status,
            commands::open_diagnostic_folder,
            commands::render_failure,
            commands::make_default_app,
            commands::apply_edit,
            commands::undo_edit,
            commands::redo_edit,
            commands::edit_state,
            commands::begin_edit_group,
            commands::end_edit_group,
            commands::save_document,
            commands::extract_pages,
            commands::merge_files,
            commands::split_document,
            commands::split_plan
        ]);
    let app = protocol::register(builder)
        .build(context)
        .expect("erreur au lancement de l'application Tauri");
    app.run(|handle, event| {
        // macOS : « Ouvrir avec » / double-clic dans le Finder n'arrive pas en argument de
        // ligne de commande mais comme événement : on transmet les chemins à l'interface.
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        if let tauri::RunEvent::Opened { urls } = &event {
            use tauri::Emitter;
            let paths: Vec<String> = file_paths(urls)
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
            if !paths.is_empty() {
                // Gardés (démarrage à froid : personne n'écoute encore) et signalés (application
                // déjà ouverte) ; l'interface vide la liste en les ouvrant.
                use tauri::Manager;
                handle.state::<PendingPaths>().push(paths.clone());
                let _ = handle.emit("open-paths", ());
            }
        }
        let _ = (handle, event);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lot 11 : le premier argument d'une seconde instance est l'exécutable lui-même (même
    /// s'il finissait par « .pdf ») ; les suivants se lisent dans son dossier courant.
    #[test]
    fn second_instance_skips_the_executable_and_reads_its_cwd() {
        let fixtures = format!("{}/../../fixtures", env!("CARGO_MANIFEST_DIR"));
        let argv = vec![
            format!("{fixtures}/simple.pdf"),
            "rotated.pdf".to_string(),
            "--flag".to_string(),
        ];
        let paths = second_instance_paths(&argv, &fixtures);
        assert_eq!(paths.len(), 1, "{paths:?}");
        assert!(paths[0].ends_with("rotated.pdf"));
    }

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

    /// Relecture du Lot 9 : les chemins reçus avant que l'interface écoute sont gardés,
    /// puis rendus une seule fois.
    #[test]
    fn pending_paths_are_kept_until_taken() {
        let pending = PendingPaths::default();
        pending.push(vec!["/a.pdf".to_string()]);
        pending.push(vec!["/b.pdf".to_string()]);
        assert_eq!(
            pending.take(),
            vec!["/a.pdf".to_string(), "/b.pdf".to_string()]
        );
        assert!(pending.take().is_empty());
    }

    /// C33 : les fichiers donnés par le Finder (URL `file://`, espaces encodés) deviennent
    /// des chemins ; une autre URL est ignorée.
    #[test]
    fn opened_urls_become_paths() {
        let (url, expected) = if cfg!(windows) {
            (
                "file:///C:/Mes%20docs/a.pdf",
                PathBuf::from(r"C:\Mes docs\a.pdf"),
            )
        } else {
            (
                "file:///tmp/Mes%20docs/a.pdf",
                PathBuf::from("/tmp/Mes docs/a.pdf"),
            )
        };
        let urls = vec![
            tauri::Url::parse(url).unwrap(),
            tauri::Url::parse("https://example.org/b.pdf").unwrap(),
        ];
        assert_eq!(file_paths(&urls), vec![expected]);
    }
}
