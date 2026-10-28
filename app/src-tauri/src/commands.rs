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

/// Annotations d'une page.
#[tauri::command]
pub async fn annotations(
    service: State<'_, ServiceHandle>,
    id: DocId,
    page: u16,
) -> Result<Vec<mega_core::Annotation>, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.annotations(id, page)).await
}

/// Annotations de toutes les pages, en une requête (Lot 9).
#[tauri::command]
pub async fn all_annotations(
    service: State<'_, ServiceHandle>,
    id: DocId,
) -> Result<Vec<Vec<mega_core::Annotation>>, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.all_annotations(id)).await
}

/// Champs de formulaire de toutes les pages, en une requête (Lot 9).
#[tauri::command]
pub async fn all_form_fields(
    service: State<'_, ServiceHandle>,
    id: DocId,
) -> Result<Vec<Vec<mega_core::FormField>>, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.all_form_fields(id)).await
}

/// Objets du contenu d'une page (Lot 5).
#[tauri::command]
pub async fn page_objects(
    service: State<'_, ServiceHandle>,
    id: DocId,
    page: u16,
) -> Result<Vec<mega_core::PageObject>, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.page_objects(id, page)).await
}

/// Champs de formulaire d'une page.
#[tauri::command]
pub async fn form_fields(
    service: State<'_, ServiceHandle>,
    id: DocId,
    page: u16,
) -> Result<Vec<mega_core::FormField>, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.form_fields(id, page)).await
}

/// Texte d'une page (Lot 6).
#[tauri::command]
pub async fn page_text(
    service: State<'_, ServiceHandle>,
    id: DocId,
    page: u16,
) -> Result<String, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.page_text(id, page)).await
}

/// Exporte des pages en images dans un dossier ; retourne les chemins écrits.
#[tauri::command]
pub async fn export_images(
    service: State<'_, ServiceHandle>,
    id: DocId,
    spec: mega_core::export::ImageExport,
    dir: String,
    prefix: String,
    force: bool,
) -> Result<Vec<String>, AppError> {
    let handle = service.inner().clone();
    let paths = blocking(handle, move |h| {
        h.export_images(id, spec, dir, prefix, force)
    })
    .await?;
    Ok(paths
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect())
}

/// Exporte le texte des pages dans un fichier.
#[tauri::command]
pub async fn export_text(
    service: State<'_, ServiceHandle>,
    id: DocId,
    pages: Option<Vec<u16>>,
    separators: bool,
    path: String,
) -> Result<(), AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.export_text(id, pages, separators, path)).await
}

/// Aperçu d'une optimisation : le rapport, sans rien modifier.
#[tauri::command]
pub async fn optimize_preview(
    service: State<'_, ServiceHandle>,
    id: DocId,
    options: mega_core::OptimizeOptions,
) -> Result<mega_core::OptimizeReport, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.optimize_preview(id, options)).await
}

/// État de sécurité du document.
#[tauri::command]
pub async fn security(
    service: State<'_, ServiceHandle>,
    id: DocId,
) -> Result<mega_core::SecurityInfo, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.security(id)).await
}

/// Pièces jointes du document.
#[tauri::command]
pub async fn attachments(
    service: State<'_, ServiceHandle>,
    id: DocId,
) -> Result<Vec<mega_core::Attachment>, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.attachments(id)).await
}

/// Écrit une pièce jointe dans un fichier choisi par l'utilisateur.
#[tauri::command]
pub async fn save_attachment(
    service: State<'_, ServiceHandle>,
    id: DocId,
    index: u16,
    path: String,
) -> Result<(), AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.save_attachment(id, index, path)).await
}

/// Compare deux pages : l'image PNG des différences est renvoyée en octets bruts, les
/// compteurs dans l'en-tête de la réponse (`tauri::ipc::Response` évite le base64).
#[tauri::command]
pub async fn compare_pages(
    service: State<'_, ServiceHandle>,
    id_a: DocId,
    page_a: u16,
    id_b: DocId,
    page_b: u16,
    scale: f32,
) -> Result<tauri::ipc::Response, AppError> {
    let handle = service.inner().clone();
    let compared = blocking(handle, move |h| {
        h.compare_pages(id_a, page_a, id_b, page_b, scale)
    })
    .await?;
    // Les compteurs sont encodés en tête du corps (24 octets, petit-boutiste), suivis du PNG :
    // le frontend les lit dans le même tampon.
    let mut body = Vec::with_capacity(compared.png.len() + 24);
    body.extend_from_slice(&compared.differing_pixels.to_le_bytes());
    body.extend_from_slice(&compared.total_pixels.to_le_bytes());
    body.extend_from_slice(&compared.width_px.to_le_bytes());
    body.extend_from_slice(&compared.height_px.to_le_bytes());
    body.extend_from_slice(&compared.png);
    Ok(tauri::ipc::Response::new(body))
}

/// Nature du formulaire et nombre de champs.
#[tauri::command]
pub async fn form_info(
    service: State<'_, ServiceHandle>,
    id: DocId,
) -> Result<mega_core::FormInfo, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.form_info(id)).await
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

/// État de Tesseract pour l'OCR : trouvé ou non, et ses langues installées.
#[tauri::command]
pub async fn ocr_status(
    service: State<'_, ServiceHandle>,
) -> Result<mega_core::ocr::OcrStatus, AppError> {
    // Pas de PDFium ici : seulement un processus `tesseract --list-langs`, hors du thread
    // de l'interface.
    let handle = service.inner().clone();
    blocking(handle, |_| Ok(mega_core::ocr::ocr_status())).await
}

/// Fichiers reçus du système (Finder) en attente d'ouverture ; la liste est vidée.
#[tauri::command]
pub fn take_pending_paths(pending: State<'_, crate::PendingPaths>) -> Vec<String> {
    pending.take()
}

/// Chemin de Tesseract choisi dans les préférences (vide ou absent : recherche automatique).
#[tauri::command]
pub fn set_tesseract_path(path: Option<String>) {
    let path = path
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .map(std::path::PathBuf::from);
    mega_core::ocr::set_tesseract_path(path);
}

/// Pour chaque page, contient-elle déjà du texte ? (choix des pages à reconnaître)
#[tauri::command]
pub async fn pages_with_text(
    service: State<'_, ServiceHandle>,
    id: DocId,
) -> Result<Vec<bool>, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.pages_with_text(id)).await
}

/// Signatures numériques du fichier ouvert, vérifiées.
#[tauri::command]
pub async fn signatures(
    service: State<'_, ServiceHandle>,
    id: DocId,
) -> Result<Vec<mega_core::digital_signature::SignatureInfo>, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| h.signatures(id)).await
}

/// Signe le document avec le certificat `p12` (mot de passe `password`) et l'enregistre dans
/// `path`. Le mot de passe ne fait que passer : il n'est ni gardé ni journalisé.
#[tauri::command]
pub async fn sign_document(
    service: State<'_, ServiceHandle>,
    id: DocId,
    path: String,
    p12: String,
    password: String,
    options: mega_core::digital_signature::SignOptions,
) -> Result<EditResult, AppError> {
    let handle = service.inner().clone();
    blocking(handle, move |h| {
        h.sign_document(id, path, p12, &password, options)
            .map(EditResult::from)
    })
    .await
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
    fn signature_info_serializes_for_the_frontend() {
        // Forme attendue par `api.ts` (`SignatureInfo`, `SignatureStatus`).
        let info = mega_core::digital_signature::signatures(
            &std::fs::read(fixture("signed-external.pdf")).unwrap(),
        )
        .unwrap();
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json[0]["status"], serde_json::json!({ "kind": "intact" }));
        assert_eq!(json[0]["field"], "Sig1");
        assert_eq!(json[0]["covers_whole_document"], true);
        assert!(json[0]["signed_at"].is_i64());
        let invalid = serde_json::to_value(mega_core::digital_signature::SignatureStatus::Invalid(
            mega_core::digital_signature::SigIssue::ContentModified,
        ))
        .unwrap();
        assert_eq!(
            invalid,
            serde_json::json!({ "kind": "invalid", "issue": "content_modified" })
        );
    }

    /// Le chemin choisi dans les préférences passe avant toute recherche ; vide, il rend la
    /// main à la recherche automatique.
    #[test]
    fn set_tesseract_path_overrides_the_search() {
        // Un fichier factice (jamais lancé) : pointer vers l'exécutable de test lui-même
        // risquerait qu'un autre test en parallèle le lance avec `--list-langs`.
        let fake = std::env::temp_dir().join(format!("mega-tesseract-{}.txt", std::process::id()));
        std::fs::write(&fake, b"pas un programme").unwrap();
        set_tesseract_path(Some(fake.to_string_lossy().into_owned()));
        assert_eq!(mega_core::ocr::find_tesseract(), Some(fake.clone()));
        set_tesseract_path(Some("   ".to_string()));
        assert_ne!(mega_core::ocr::find_tesseract(), Some(fake.clone()));
        let _ = std::fs::remove_file(&fake);
    }

    #[test]
    fn ocr_status_serializes_for_the_frontend() {
        let json = serde_json::to_value(mega_core::ocr::ocr_status()).unwrap();
        assert!(json["available"].is_boolean());
        assert!(json["languages"].is_array());
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
