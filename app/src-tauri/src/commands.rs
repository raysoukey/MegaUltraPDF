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
    if lock_registry(&registry)?.remove(id) {
        Ok(())
    } else {
        Err(AppError::new(
            ErrorKind::NotFound,
            format!("document {id} inconnu"),
        ))
    }
}
