// SPDX-License-Identifier: GPL-3.0-or-later
//! Commandes Tauri appelées par le frontend via `invoke`.
//!
//! \[Apprentissage\] `#[tauri::command]` transforme une fonction Rust ordinaire en point
//! d'entrée appelable depuis JavaScript. Les arguments sont désérialisés depuis le JSON
//! (`path: String`), la valeur de retour sérialisée en JSON. `State<'_, T>` donne accès
//! à une valeur partagée enregistrée avec `.manage(...)` au démarrage.

use std::sync::Mutex;

use mega_core::{Document, PageSize};
use serde::Serialize;
use tauri::State;

use crate::error::{AppError, ErrorKind};
use crate::registry::{DocId, DocumentRegistry};

/// Ce que le frontend sait d'un document ouvert (reflété par `DocumentInfo` dans `api.ts`).
#[derive(Debug, Serialize)]
pub struct DocumentInfo {
    pub id: DocId,
    pub path: String,
    pub title: Option<String>,
    pub page_count: u16,
    pub pages: Vec<PageSize>,
}

/// État partagé de l'application : le registre, protégé par un verrou.
pub type SharedRegistry = Mutex<DocumentRegistry>;

/// Verrouille le registre ; un verrou « empoisonné » (panique d'un autre thread) est
/// traité comme une erreur interne plutôt qu'une nouvelle panique.
pub fn lock_registry(
    registry: &SharedRegistry,
) -> Result<std::sync::MutexGuard<'_, DocumentRegistry>, AppError> {
    registry
        .lock()
        .map_err(|_| AppError::new(ErrorKind::Internal, "registre des documents indisponible"))
}

/// Ouvre le fichier `path` et l'enregistre ; retourne sa description.
#[tauri::command]
pub fn open_document(
    registry: State<'_, SharedRegistry>,
    path: String,
    password: Option<String>,
) -> Result<DocumentInfo, AppError> {
    let doc = Document::open(&path, password.as_deref())?;
    let info = DocumentInfo {
        id: 0,
        path: path.clone(),
        title: doc.title(),
        page_count: doc.page_count(),
        pages: doc.page_sizes()?,
    };
    let id = lock_registry(&registry)?.insert(doc);
    Ok(DocumentInfo { id, ..info })
}

/// Ferme le document `id`.
#[tauri::command]
pub fn close_document(registry: State<'_, SharedRegistry>, id: DocId) -> Result<(), AppError> {
    close_in(&registry, id)
}

/// Logique de `close_document`, séparée de la commande pour être testable sans Tauri.
pub fn close_in(registry: &SharedRegistry, id: DocId) -> Result<(), AppError> {
    if lock_registry(registry)?.remove(id) {
        Ok(())
    } else {
        Err(AppError::new(
            ErrorKind::NotFound,
            format!("document {id} inconnu"),
        ))
    }
}

/// Fichier passé en argument de ligne de commande au lancement, s'il y en a un.
///
/// Permet `mega-app fichier.pdf` (et, en développement, `pnpm tauri dev -- -- fichier.pdf`).
/// Le frontend l'appelle au démarrage et ouvre le document s'il existe.
#[tauri::command]
pub fn startup_file() -> Option<String> {
    // `args_os` plutôt que `args` : ce dernier panique sur un argument qui n'est pas de
    // l'Unicode valide ; ici un tel argument est simplement ignoré.
    startup_file_from_args(
        std::env::args_os()
            .skip(1)
            .filter_map(|a| a.into_string().ok()),
    )
}

/// Premier argument qui ressemble à un fichier PDF existant.
pub fn startup_file_from_args(args: impl IntoIterator<Item = String>) -> Option<String> {
    args.into_iter().find(|a| {
        !a.starts_with('-')
            && a.to_ascii_lowercase().ends_with(".pdf")
            && std::path::Path::new(a).is_file()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        format!("{}/../../fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn close_document_unknown_id_is_not_found() {
        let registry: SharedRegistry = Mutex::new(DocumentRegistry::default());
        let err = close_in(&registry, 42).unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound);
    }

    #[test]
    fn startup_file_picks_first_existing_pdf_argument() {
        let args = vec![
            "--flag".to_string(),
            "inexistant.pdf".to_string(),
            fixture("simple.pdf"),
        ];
        assert_eq!(startup_file_from_args(args), Some(fixture("simple.pdf")));
        assert_eq!(startup_file_from_args(Vec::<String>::new()), None);
    }
}
