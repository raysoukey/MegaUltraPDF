// SPDX-License-Identifier: GPL-3.0-or-later
//! Commandes Tauri appelées par le frontend via `invoke`.
//!
//! \[Apprentissage\] `#[tauri::command]` transforme une fonction Rust en point d'entrée
//! appelable depuis JavaScript. Les commandes sont `async` : Tauri les exécute sur son
//! runtime `tokio`, hors du thread de l'interface. Comme le [`ServiceHandle`] **bloque**
//! en attendant l'acteur PDFium, chaque commande délègue à `spawn_blocking`, un pool de
//! threads prévu pour les appels bloquants, puis `.await` le résultat. L'état partagé est
//! le handle lui-même (clonable), enregistré avec `.manage(...)` au démarrage.

use mega_core::service::{DocId, DocumentSummary, ServiceHandle};
use mega_core::{
    DocumentProperties, EditCommand, EditState, OutlineNode, PageLink, SearchHit, SearchOptions,
    TextBox,
};
use tauri::State;

use crate::error::{AppError, ErrorKind};

/// Exécute une opération bloquante du service sur le pool `spawn_blocking` et rapatrie
/// le résultat dans le monde `async`.
async fn blocking<T: Send + 'static>(
    handle: ServiceHandle,
    op: impl FnOnce(ServiceHandle) -> mega_core::CoreResult<T> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(move || op(handle))
        .await
        .map_err(|e| AppError::new(ErrorKind::Internal, format!("tâche interrompue : {e}")))?
        .map_err(AppError::from)
}

/// Ouvre le fichier `path` dans l'acteur et retourne sa description.
#[tauri::command]
pub async fn open_document(
    service: State<'_, ServiceHandle>,
    path: String,
    password: Option<String>,
) -> Result<DocumentSummary, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.open_path(path, password.as_deref())).await
}

/// Ferme le document `id`.
#[tauri::command]
pub async fn close_document(service: State<'_, ServiceHandle>, id: DocId) -> Result<(), AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.close(id)).await
}

/// Déclare la génération courante du document (décidée par le frontend) ; retourne la
/// valeur retenue par l'acteur (qui ne recule jamais).
#[tauri::command]
pub async fn set_generation(
    service: State<'_, ServiceHandle>,
    id: DocId,
    generation: u64,
) -> Result<u64, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.set_generation(id, generation)).await
}

/// Caractères positionnés d'une page (calque de texte).
#[tauri::command]
pub async fn text_layer(
    service: State<'_, ServiceHandle>,
    id: DocId,
    page: u16,
) -> Result<Vec<TextBox>, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.text_boxes(id, page)).await
}

/// Recherche dans tout le document.
#[tauri::command]
pub async fn search_document(
    service: State<'_, ServiceHandle>,
    id: DocId,
    query: String,
    match_case: bool,
    whole_word: bool,
) -> Result<Vec<SearchHit>, AppError> {
    let handle = service.inner().clone();
    let options = SearchOptions {
        match_case,
        whole_word,
    };
    blocking(handle, move |h| h.search(id, &query, options)).await
}

/// Signets du document.
#[tauri::command]
pub async fn outline(
    service: State<'_, ServiceHandle>,
    id: DocId,
) -> Result<Vec<OutlineNode>, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.outline(id)).await
}

/// Liens d'une page.
#[tauri::command]
pub async fn page_links(
    service: State<'_, ServiceHandle>,
    id: DocId,
    page: u16,
) -> Result<Vec<PageLink>, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.links(id, page)).await
}

/// Propriétés (métadonnées, version) du document.
#[tauri::command]
pub async fn document_properties(
    service: State<'_, ServiceHandle>,
    id: DocId,
) -> Result<DocumentProperties, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.properties(id)).await
}

/// Résultat d'une édition : le document mis à jour et l'état de l'historique.
#[derive(Debug, serde::Serialize)]
pub struct EditResult {
    /// Résumé à jour (nombre et tailles des pages peuvent avoir changé).
    pub summary: DocumentSummary,
    /// Annuler / rétablir possibles, modifié, libellés.
    pub state: EditState,
}

impl From<(DocumentSummary, EditState)> for EditResult {
    fn from((summary, state): (DocumentSummary, EditState)) -> Self {
        EditResult { summary, state }
    }
}

/// Applique une commande d'édition (pivoter, supprimer, déplacer, insérer…).
#[tauri::command]
pub async fn apply_edit(
    service: State<'_, ServiceHandle>,
    id: DocId,
    command: EditCommand,
) -> Result<EditResult, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.apply(id, command).map(EditResult::from)).await
}

/// Annule la dernière édition.
#[tauri::command]
pub async fn undo_edit(
    service: State<'_, ServiceHandle>,
    id: DocId,
) -> Result<EditResult, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.undo(id).map(EditResult::from)).await
}

/// Rétablit la dernière édition annulée.
#[tauri::command]
pub async fn redo_edit(
    service: State<'_, ServiceHandle>,
    id: DocId,
) -> Result<EditResult, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.redo(id).map(EditResult::from)).await
}

/// État de l'historique d'un document.
#[tauri::command]
pub async fn edit_state(
    service: State<'_, ServiceHandle>,
    id: DocId,
) -> Result<EditState, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.edit_state(id)).await
}

/// Enregistre le document à son chemin, ou dans `path` s'il est fourni (« Enregistrer sous »).
#[tauri::command]
pub async fn save_document(
    service: State<'_, ServiceHandle>,
    id: DocId,
    path: Option<String>,
) -> Result<EditResult, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| {
        match path {
            Some(p) => h.save_as(id, p),
            None => h.save(id),
        }
        .map(EditResult::from)
    })
    .await
}

/// Écrit les pages choisies dans un nouveau fichier.
#[tauri::command]
pub async fn extract_pages(
    service: State<'_, ServiceHandle>,
    id: DocId,
    pages: Vec<u16>,
    path: String,
) -> Result<(), AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.extract_pages(id, pages, path)).await
}

/// Fusionne des fichiers ; le résultat est ouvert comme nouveau document.
#[tauri::command]
pub async fn merge_files(
    service: State<'_, ServiceHandle>,
    paths: Vec<String>,
) -> Result<DocumentSummary, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| {
        h.merge_files(paths.into_iter().map(Into::into).collect())
    })
    .await
}

/// Scinde le document en fichiers `<base_name>-N.pdf` ; retourne les chemins créés.
#[tauri::command]
pub async fn split_document(
    service: State<'_, ServiceHandle>,
    id: DocId,
    parts: Vec<Vec<u16>>,
    out_dir: String,
    base_name: String,
) -> Result<Vec<String>, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| {
        h.split_document(id, parts, out_dir, &base_name)
            .map(|paths| paths.iter().map(|p| p.display().to_string()).collect())
    })
    .await
}

/// Prévisualise une scission : les pages de chaque partie, sans rien écrire. La règle
/// (plages, toutes les N, signets) vit dans `mega_core::pages::plan_split`.
#[tauri::command]
pub async fn split_plan(
    service: State<'_, ServiceHandle>,
    id: DocId,
    mode: mega_core::pages::SplitMode,
) -> Result<Vec<Vec<u16>>, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.split_plan(id, mode)).await
}

/// Taille d'un fichier en octets (pour le dialogue des propriétés).
#[tauri::command]
pub async fn file_size(path: String) -> Result<u64, AppError> {
    tauri::async_runtime::spawn_blocking(move || std::fs::metadata(&path).map(|m| m.len()))
        .await
        .map_err(|e| AppError::new(ErrorKind::Internal, format!("tâche interrompue : {e}")))?
        .map_err(|e| AppError::new(ErrorKind::Io, format!("Erreur d'accès au fichier : {e}")))
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
    fn startup_file_picks_first_existing_pdf_argument() {
        let args = vec![
            "--flag".to_string(),
            "inexistant.pdf".to_string(),
            fixture("simple.pdf"),
        ];
        assert_eq!(startup_file_from_args(args), Some(fixture("simple.pdf")));
        assert_eq!(startup_file_from_args(Vec::<String>::new()), None);
    }

    #[tokio::test]
    async fn blocking_maps_core_errors_to_app_errors() {
        let handle = mega_core::service::PdfService::spawn();
        let err = blocking(handle, |h| h.close(42)).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound);
    }
}
