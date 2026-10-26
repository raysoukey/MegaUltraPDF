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

/// Façon de découper un document (miroir TypeScript : `SplitMode`).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SplitMode {
    /// Plages explicites (« 1-2,3-5 »), une partie par plage.
    Ranges {
        /// La spécification saisie par l'utilisateur.
        spec: String,
    },
    /// Une partie toutes les `n` pages.
    Every {
        /// Taille d'une partie (≥ 1).
        n: u16,
    },
    /// Une partie par signet de premier niveau.
    Bookmarks,
}

/// Les parties (index de pages) d'une scission, calculées en Rust pour que la règle
/// d'analyse des plages reste unique (`PageSpec`).
fn plan_split(
    page_count: u16,
    outline: &[mega_core::OutlineNode],
    mode: SplitMode,
) -> Result<Vec<Vec<u16>>, AppError> {
    use mega_core::pagespec::PageSpec;
    match mode {
        SplitMode::Ranges { spec } => Ok(PageSpec::ranges(&spec, page_count)?),
        SplitMode::Every { n } => {
            if n == 0 {
                return Err(AppError::new(
                    ErrorKind::InvalidOperation,
                    "la taille d'une partie doit être d'au moins une page",
                ));
            }
            Ok(PageSpec::every(n, page_count))
        }
        SplitMode::Bookmarks => {
            let starts: Vec<u16> = outline.iter().filter_map(|n| n.page).collect();
            if starts.is_empty() {
                return Err(AppError::new(
                    ErrorKind::InvalidOperation,
                    "ce document n'a pas de signets",
                ));
            }
            Ok(PageSpec::split_at(&starts, page_count))
        }
    }
}

/// Prévisualise une scission : les pages de chaque partie, sans rien écrire.
#[tauri::command]
pub async fn split_plan(
    service: State<'_, ServiceHandle>,
    id: DocId,
    mode: SplitMode,
) -> Result<Vec<Vec<u16>>, AppError> {
    let handle = service.inner().clone();
    let needs_outline = matches!(mode, SplitMode::Bookmarks);
    let (page_count, outline) = blocking(handle, move |h| {
        let summary = h.summary(id)?;
        let outline = if needs_outline {
            h.outline(id)?
        } else {
            Vec::new()
        };
        Ok((summary.page_count, outline))
    })
    .await?;
    plan_split(page_count, &outline, mode)
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

    fn node(title: &str, page: Option<u16>) -> mega_core::OutlineNode {
        mega_core::OutlineNode {
            title: title.to_string(),
            page,
            children: vec![],
        }
    }

    #[test]
    fn plan_split_by_ranges_every_and_bookmarks() {
        let outline = vec![
            node("A", Some(0)),
            node("B", Some(2)),
            node("sans page", None),
        ];
        assert_eq!(
            plan_split(
                5,
                &outline,
                SplitMode::Ranges {
                    spec: "1-2,4-".into()
                }
            )
            .unwrap(),
            vec![vec![0, 1], vec![3, 4]]
        );
        assert_eq!(
            plan_split(5, &outline, SplitMode::Every { n: 2 }).unwrap(),
            vec![vec![0, 1], vec![2, 3], vec![4]]
        );
        assert_eq!(
            plan_split(5, &outline, SplitMode::Bookmarks).unwrap(),
            vec![vec![0, 1], vec![2, 3, 4]]
        );
    }

    #[test]
    fn plan_split_rejects_bad_input() {
        assert_eq!(
            plan_split(5, &[], SplitMode::Ranges { spec: "9".into() })
                .unwrap_err()
                .kind,
            ErrorKind::InvalidOperation
        );
        assert_eq!(
            plan_split(5, &[], SplitMode::Every { n: 0 })
                .unwrap_err()
                .kind,
            ErrorKind::InvalidOperation
        );
        assert_eq!(
            plan_split(5, &[], SplitMode::Bookmarks).unwrap_err().kind,
            ErrorKind::InvalidOperation
        );
    }

    #[tokio::test]
    async fn blocking_maps_core_errors_to_app_errors() {
        let handle = mega_core::service::PdfService::spawn();
        let err = blocking(handle, |h| h.close(42)).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound);
    }
}
