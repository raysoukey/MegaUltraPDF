// SPDX-License-Identifier: GPL-3.0-or-later
//! L'acteur PDFium : un thread unique possède tous les documents ouverts.
//!
//! \[Apprentissage\] **Patron acteur.** Plutôt que de partager les documents entre threads
//! derrière des verrous, un seul thread (« l'acteur ») les possède et traite des **messages**
//! reçus par un canal. Les autres threads ne manipulent qu'un [`ServiceHandle`] : ils
//! envoient une requête avec, à l'intérieur, un canal de réponse à usage unique, puis
//! attendent cette réponse. Avantages : PDFium n'est jamais appelée depuis deux threads,
//! on peut ordonner les requêtes (pages visibles d'abord) et en annuler (zoom périmé).
//!
//! Canaux utilisés : `std::sync::mpsc::channel` pour les requêtes (plusieurs émetteurs, un
//! récepteur) et `std::sync::mpsc::sync_channel(1)` comme « oneshot » pour chaque réponse.

mod cache;
mod queue;

use std::cell::RefCell;
use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};

use serde::{Deserialize, Serialize};

pub use cache::DocId;
pub use queue::Priority;

use crate::document::{Document, PageSize};
use crate::edit::{EditCommand, EditState};
use crate::error::{CoreError, CoreResult};
use crate::geometry::Rotation;
use crate::links::PageLink;
use crate::outline::OutlineNode;
use crate::properties::DocumentProperties;
use crate::text::{SearchHit, SearchOptions, TextBox};
use cache::{PageKey, PngCache};
use queue::PriorityQueue;

/// Budget par défaut du cache de pages rendues : 256 Mo de PNG.
pub const DEFAULT_CACHE_BUDGET: usize = 256 * 1024 * 1024;

/// Ce que le frontend sait d'un document ouvert.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentSummary {
    /// Identifiant à utiliser dans toutes les requêtes suivantes.
    pub id: DocId,
    /// Chemin du fichier, s'il vient d'un fichier.
    pub path: Option<String>,
    /// Titre des métadonnées.
    pub title: Option<String>,
    /// Nombre de pages.
    pub page_count: u16,
    /// Dimensions de chaque page, en points.
    pub pages: Vec<PageSize>,
    /// Génération courante (0 à l'ouverture).
    pub generation: u64,
}

/// Une demande de rendu de page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderRequest {
    /// Document.
    pub doc: DocId,
    /// Page (à partir de 0).
    pub page: u16,
    /// Échelle (1.0 = 72 px par pouce).
    pub scale: f32,
    /// Rotation d'affichage.
    pub rotation: Rotation,
    /// Génération du document au moment de la demande ; périmée → `Cancelled`.
    pub generation: u64,
    /// Priorité dans la file.
    pub priority: Priority,
}

/// Les messages que l'acteur comprend. Chaque variante porte son canal de réponse.
///
/// \[Apprentissage\] Une `enum` par message est le cœur du patron : le `match` de l'acteur
/// est exhaustif, le compilateur signale tout message oublié.
enum Request {
    /// Requête de test qui touche le document `id` puis panique (R4, Lot 13).
    #[cfg(test)]
    Panic {
        id: DocId,
        reply: SyncSender<CoreResult<()>>,
    },
    OpenPath {
        path: PathBuf,
        password: Option<String>,
        reply: SyncSender<CoreResult<DocumentSummary>>,
    },
    OpenBytes {
        bytes: Vec<u8>,
        password: Option<String>,
        reply: SyncSender<CoreResult<DocumentSummary>>,
    },
    Close {
        id: DocId,
        reply: SyncSender<CoreResult<()>>,
    },
    Summary {
        id: DocId,
        reply: SyncSender<CoreResult<DocumentSummary>>,
    },
    SetGeneration {
        id: DocId,
        generation: u64,
        reply: SyncSender<CoreResult<u64>>,
    },
    Render {
        request: RenderRequest,
        reply: SyncSender<CoreResult<Arc<Vec<u8>>>>,
    },
    TextBoxes {
        id: DocId,
        page: u16,
        reply: SyncSender<CoreResult<Vec<TextBox>>>,
    },
    Search {
        id: DocId,
        query: String,
        options: SearchOptions,
        reply: SyncSender<CoreResult<Vec<SearchHit>>>,
    },
    Outline {
        id: DocId,
        reply: SyncSender<CoreResult<Vec<OutlineNode>>>,
    },
    /// Annotations d'une page.
    Annotations {
        id: DocId,
        page: u16,
        reply: SyncSender<CoreResult<Vec<crate::annotations::Annotation>>>,
    },
    AllAnnotations {
        id: DocId,
        reply: SyncSender<CoreResult<Vec<Vec<crate::annotations::Annotation>>>>,
    },
    /// Objets du contenu d'une page (Lot 5).
    PageObjects {
        id: DocId,
        page: u16,
        reply: SyncSender<CoreResult<Vec<crate::content::PageObject>>>,
    },
    Links {
        id: DocId,
        page: u16,
        reply: SyncSender<CoreResult<Vec<PageLink>>>,
    },
    Properties {
        id: DocId,
        reply: SyncSender<CoreResult<DocumentProperties>>,
    },
    Apply {
        id: DocId,
        command: EditCommand,
        reply: SyncSender<CoreResult<(DocumentSummary, EditState)>>,
    },
    Undo {
        id: DocId,
        reply: SyncSender<CoreResult<(DocumentSummary, EditState)>>,
    },
    Redo {
        id: DocId,
        reply: SyncSender<CoreResult<(DocumentSummary, EditState)>>,
    },
    EditState {
        id: DocId,
        reply: SyncSender<CoreResult<EditState>>,
    },
    BeginOcrGroup {
        id: DocId,
        pages: usize,
        reply: SyncSender<CoreResult<()>>,
    },
    EndEditGroup {
        id: DocId,
        reply: SyncSender<CoreResult<EditState>>,
    },
    Save {
        id: DocId,
        path: Option<PathBuf>,
        reply: SyncSender<CoreResult<(DocumentSummary, EditState)>>,
    },
    ExtractPages {
        id: DocId,
        pages: Vec<u16>,
        path: PathBuf,
        reply: SyncSender<CoreResult<()>>,
    },
    MergeFiles {
        paths: Vec<PathBuf>,
        reply: SyncSender<CoreResult<DocumentSummary>>,
    },
    SplitDocument {
        id: DocId,
        parts: Vec<Vec<u16>>,
        out_dir: PathBuf,
        base_name: String,
        reply: SyncSender<CoreResult<Vec<PathBuf>>>,
    },
    /// Champs de formulaire d'une page.
    FormFields {
        id: DocId,
        page: u16,
        reply: SyncSender<CoreResult<Vec<crate::forms::FormField>>>,
    },
    AllFormFields {
        id: DocId,
        reply: SyncSender<CoreResult<Vec<Vec<crate::forms::FormField>>>>,
    },
    /// Nature et taille du formulaire.
    FormInfo {
        id: DocId,
        reply: SyncSender<CoreResult<crate::forms::FormInfo>>,
    },
    /// Texte d'une page (Lot 6).
    PageText {
        id: DocId,
        page: u16,
        reply: SyncSender<CoreResult<String>>,
    },
    /// Export d'images dans un dossier.
    ExportImages {
        id: DocId,
        spec: crate::export::ImageExport,
        dir: PathBuf,
        prefix: String,
        force: bool,
        reply: SyncSender<CoreResult<Vec<PathBuf>>>,
    },
    /// Export du texte dans un fichier.
    ExportText {
        id: DocId,
        pages: Option<Vec<u16>>,
        separators: bool,
        path: PathBuf,
        reply: SyncSender<CoreResult<()>>,
    },
    /// Aperçu d'une optimisation (rien n'est modifié).
    OptimizePreview {
        id: DocId,
        options: crate::optimize::OptimizeOptions,
        reply: SyncSender<CoreResult<crate::optimize::OptimizeReport>>,
    },
    /// État de sécurité.
    Security {
        id: DocId,
        reply: SyncSender<CoreResult<crate::security::SecurityInfo>>,
    },
    /// Pour chaque page, contient-elle déjà du texte ? (choix des pages à reconnaître)
    #[cfg(feature = "ocr")]
    PagesWithText {
        id: DocId,
        reply: SyncSender<CoreResult<Vec<bool>>>,
    },
    /// Signatures numériques du fichier, vérifiées.
    #[cfg(feature = "signature")]
    Signatures {
        id: DocId,
        reply: SyncSender<CoreResult<Vec<crate::digital_signature::SignatureInfo>>>,
    },
    /// Signe le document avec l'identité d'un fichier PKCS#12 et l'enregistre dans `path`.
    #[cfg(feature = "signature")]
    SignDocument {
        id: DocId,
        path: PathBuf,
        p12: PathBuf,
        password: String,
        options: crate::digital_signature::SignOptions,
        reply: SyncSender<CoreResult<(DocumentSummary, EditState)>>,
    },
    /// Pièces jointes.
    Attachments {
        id: DocId,
        reply: SyncSender<CoreResult<Vec<crate::attachments::Attachment>>>,
    },
    /// Extraction d'une pièce jointe vers un fichier.
    SaveAttachment {
        id: DocId,
        index: u16,
        path: PathBuf,
        reply: SyncSender<CoreResult<()>>,
    },
    /// Image des différences entre deux pages (PNG) et compteurs.
    ComparePages {
        id_a: DocId,
        page_a: u16,
        id_b: DocId,
        page_b: u16,
        scale: f32,
        reply: SyncSender<CoreResult<ComparedPages>>,
    },
    /// Prévisualise une scission (parties calculées, rien d'écrit).
    SplitPlan {
        id: DocId,
        mode: crate::pages::SplitMode,
        reply: SyncSender<CoreResult<Vec<Vec<u16>>>>,
    },
}

impl Request {
    /// Priorité de traitement : les rendus portent la leur, tout le reste est urgent
    /// (ouvrir, fermer, chercher… ne doit pas attendre derrière des vignettes).
    fn priority(&self) -> Priority {
        match self {
            Request::Render { request, .. } => request.priority,
            _ => Priority::Visible,
        }
    }
}

/// Un document tel que l'acteur le garde.
struct Entry {
    doc: Document,
    generation: u64,
}

/// Image des différences entre deux pages, prête à traverser la frontière (PNG + compteurs).
#[derive(Debug, Clone)]
pub struct ComparedPages {
    /// L'image PNG.
    pub png: Vec<u8>,
    /// Pixels différents.
    pub differing_pixels: u64,
    /// Pixels de l'image.
    pub total_pixels: u64,
    /// Largeur de l'image.
    pub width_px: u32,
    /// Hauteur de l'image.
    pub height_px: u32,
}

/// Le service : construit l'acteur et rend le premier handle.
pub struct PdfService;

impl PdfService {
    /// Démarre l'acteur avec le budget de cache par défaut.
    pub fn spawn() -> ServiceHandle {
        Self::spawn_with(DEFAULT_CACHE_BUDGET)
    }

    /// Démarre l'acteur avec un budget de cache en octets.
    pub fn spawn_with(cache_budget_bytes: usize) -> ServiceHandle {
        let (tx, rx) = mpsc::channel::<Request>();
        // Le thread meurt de lui-même quand tous les `Sender` (donc tous les handles) sont
        // détruits : `recv()` renvoie alors `Err` et la boucle se termine.
        std::thread::Builder::new()
            .name("pdfium-actor".to_string())
            .spawn(move || Actor::new(cache_budget_bytes).run(rx))
            .map(|_| ())
            .unwrap_or_else(|e| eprintln!("impossible de démarrer l'acteur PDFium : {e}"));
        ServiceHandle { tx }
    }
}

/// Poignée clonable vers l'acteur. Toutes les méthodes bloquent jusqu'à la réponse.
#[derive(Clone)]
pub struct ServiceHandle {
    tx: Sender<Request>,
}

impl ServiceHandle {
    /// Envoie une requête construite par `make` (qui reçoit le canal de réponse) et attend.
    fn call<T>(&self, make: impl FnOnce(SyncSender<CoreResult<T>>) -> Request) -> CoreResult<T> {
        // [Apprentissage] `sync_channel(1)` : un canal borné à une place créé pour cette
        // seule requête. L'émetteur part dans le message, le récepteur reste ici : c'est
        // un « oneshot » avec les outils de la bibliothèque standard.
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.tx
            .send(make(reply_tx))
            .map_err(|_| CoreError::ServiceStopped)?;
        // Canal de réponse fermé sans réponse : la requête a paniqué dans l'acteur (qui, lui,
        // continue : voir `Actor::step`).
        reply_rx.recv().map_err(|_| CoreError::Aborted)?
    }

    /// Ouvre un fichier.
    pub fn open_path(
        &self,
        path: impl Into<PathBuf>,
        password: Option<&str>,
    ) -> CoreResult<DocumentSummary> {
        let (path, password) = (path.into(), password.map(str::to_string));
        self.call(|reply| Request::OpenPath {
            path,
            password,
            reply,
        })
    }

    /// Ouvre des octets déjà en mémoire.
    pub fn open_bytes(
        &self,
        bytes: Vec<u8>,
        password: Option<&str>,
    ) -> CoreResult<DocumentSummary> {
        let password = password.map(str::to_string);
        self.call(|reply| Request::OpenBytes {
            bytes,
            password,
            reply,
        })
    }

    /// Ferme un document (libère PDFium et le cache).
    pub fn close(&self, id: DocId) -> CoreResult<()> {
        self.call(|reply| Request::Close { id, reply })
    }

    /// Résumé d'un document ouvert.
    pub fn summary(&self, id: DocId) -> CoreResult<DocumentSummary> {
        self.call(|reply| Request::Summary { id, reply })
    }

    /// Déclare la génération courante du document, décidée par le frontend (incrémentée à
    /// chaque changement de zoom ou de rotation). Les rendus demandés avec une génération
    /// plus ancienne seront annulés. Le compteur ne recule jamais ; retourne sa valeur.
    ///
    /// Le cache n'est **pas** vidé : sa clé contient déjà l'échelle et la rotation, et le
    /// test de génération précède la consultation du cache. Revenir à un zoom précédent
    /// réutilise donc les images déjà rendues.
    pub fn set_generation(&self, id: DocId, generation: u64) -> CoreResult<u64> {
        self.call(|reply| Request::SetGeneration {
            id,
            generation,
            reply,
        })
    }

    /// Rend une page en PNG (depuis le cache si possible).
    pub fn render_png(&self, request: RenderRequest) -> CoreResult<Arc<Vec<u8>>> {
        self.call(|reply| Request::Render { request, reply })
    }

    /// Caractères positionnés d'une page.
    pub fn text_boxes(&self, id: DocId, page: u16) -> CoreResult<Vec<TextBox>> {
        self.call(|reply| Request::TextBoxes { id, page, reply })
    }

    /// Recherche dans tout le document, résultats dans l'ordre des pages.
    pub fn search(
        &self,
        id: DocId,
        query: &str,
        options: SearchOptions,
    ) -> CoreResult<Vec<SearchHit>> {
        let query = query.to_string();
        self.call(|reply| Request::Search {
            id,
            query,
            options,
            reply,
        })
    }

    /// Signets du document.
    pub fn outline(&self, id: DocId) -> CoreResult<Vec<OutlineNode>> {
        self.call(|reply| Request::Outline { id, reply })
    }

    /// Annotations d'une page (voir `Document::annotations`).
    pub fn annotations(
        &self,
        id: DocId,
        page: u16,
    ) -> CoreResult<Vec<crate::annotations::Annotation>> {
        self.call(|reply| Request::Annotations { id, page, reply })
    }

    /// Annotations de toutes les pages (voir `Document::all_annotations`).
    pub fn all_annotations(
        &self,
        id: DocId,
    ) -> CoreResult<Vec<Vec<crate::annotations::Annotation>>> {
        self.call(|reply| Request::AllAnnotations { id, reply })
    }

    /// Objets du contenu d'une page (voir `Document::page_objects`).
    pub fn page_objects(
        &self,
        id: DocId,
        page: u16,
    ) -> CoreResult<Vec<crate::content::PageObject>> {
        self.call(|reply| Request::PageObjects { id, page, reply })
    }

    /// Liens d'une page.
    pub fn links(&self, id: DocId, page: u16) -> CoreResult<Vec<PageLink>> {
        self.call(|reply| Request::Links { id, page, reply })
    }

    /// Propriétés du document.
    pub fn properties(&self, id: DocId) -> CoreResult<DocumentProperties> {
        self.call(|reply| Request::Properties { id, reply })
    }

    /// Applique une édition ; retourne le document mis à jour et l'état de l'historique.
    pub fn apply(
        &self,
        id: DocId,
        command: EditCommand,
    ) -> CoreResult<(DocumentSummary, EditState)> {
        self.call(|reply| Request::Apply { id, command, reply })
    }

    /// Annule la dernière édition.
    pub fn undo(&self, id: DocId) -> CoreResult<(DocumentSummary, EditState)> {
        self.call(|reply| Request::Undo { id, reply })
    }

    /// Rétablit la dernière édition annulée.
    pub fn redo(&self, id: DocId) -> CoreResult<(DocumentSummary, EditState)> {
        self.call(|reply| Request::Redo { id, reply })
    }

    /// État de l'historique (modifié, annuler/rétablir possibles, libellés).
    pub fn edit_state(&self, id: DocId) -> CoreResult<EditState> {
        self.call(|reply| Request::EditState { id, reply })
    }

    /// Ouvre le groupe d'éditions d'une reconnaissance de texte sur `pages` pages : un seul
    /// pas d'annulation pour toutes (voir [`Document::begin_edit_group`]).
    pub fn begin_edit_group(&self, id: DocId, pages: usize) -> CoreResult<()> {
        self.call(|reply| Request::BeginOcrGroup { id, pages, reply })
    }

    /// Ferme le groupe d'éditions ouvert et rend l'état de l'historique.
    pub fn end_edit_group(&self, id: DocId) -> CoreResult<EditState> {
        self.call(|reply| Request::EndEditGroup { id, reply })
    }

    /// Enregistre à son chemin d'origine.
    pub fn save(&self, id: DocId) -> CoreResult<(DocumentSummary, EditState)> {
        self.call(|reply| Request::Save {
            id,
            path: None,
            reply,
        })
    }

    /// Enregistre sous un nouveau chemin (qui devient le chemin du document).
    pub fn save_as(
        &self,
        id: DocId,
        path: impl Into<PathBuf>,
    ) -> CoreResult<(DocumentSummary, EditState)> {
        let path = Some(path.into());
        self.call(|reply| Request::Save { id, path, reply })
    }

    /// Écrit les pages `pages` dans un nouveau fichier `path` (le document n'est pas modifié).
    pub fn extract_pages(
        &self,
        id: DocId,
        pages: Vec<u16>,
        path: impl Into<PathBuf>,
    ) -> CoreResult<()> {
        let path = path.into();
        self.call(|reply| Request::ExtractPages {
            id,
            pages,
            path,
            reply,
        })
    }

    /// Fusionne des fichiers et ouvre le résultat comme nouveau document (sans chemin).
    pub fn merge_files(&self, paths: Vec<PathBuf>) -> CoreResult<DocumentSummary> {
        self.call(|reply| Request::MergeFiles { paths, reply })
    }

    /// Scinde le document en fichiers `<base_name>-N.pdf` dans `out_dir` ; retourne leurs chemins.
    pub fn split_document(
        &self,
        id: DocId,
        parts: Vec<Vec<u16>>,
        out_dir: impl Into<PathBuf>,
        base_name: &str,
    ) -> CoreResult<Vec<PathBuf>> {
        let (out_dir, base_name) = (out_dir.into(), base_name.to_string());
        self.call(|reply| Request::SplitDocument {
            id,
            parts,
            out_dir,
            base_name,
            reply,
        })
    }

    /// Champs de formulaire d'une page (voir `Document::form_fields`).
    pub fn form_fields(&self, id: DocId, page: u16) -> CoreResult<Vec<crate::forms::FormField>> {
        self.call(|reply| Request::FormFields { id, page, reply })
    }

    /// Champs de toutes les pages (voir `Document::all_form_fields`).
    pub fn all_form_fields(&self, id: DocId) -> CoreResult<Vec<Vec<crate::forms::FormField>>> {
        self.call(|reply| Request::AllFormFields { id, reply })
    }

    /// Nature du formulaire et nombre de champs.
    pub fn form_info(&self, id: DocId) -> CoreResult<crate::forms::FormInfo> {
        self.call(|reply| Request::FormInfo { id, reply })
    }

    /// Texte d'une page (voir `Document::page_text`).
    pub fn page_text(&self, id: DocId, page: u16) -> CoreResult<String> {
        self.call(|reply| Request::PageText { id, page, reply })
    }

    /// Export d'images (voir `Document::export_images`).
    pub fn export_images(
        &self,
        id: DocId,
        spec: crate::export::ImageExport,
        dir: impl Into<PathBuf>,
        prefix: String,
        force: bool,
    ) -> CoreResult<Vec<PathBuf>> {
        let dir = dir.into();
        self.call(|reply| Request::ExportImages {
            id,
            spec,
            dir,
            prefix,
            force,
            reply,
        })
    }

    /// Export du texte dans un fichier (voir `Document::export_text`).
    pub fn export_text(
        &self,
        id: DocId,
        pages: Option<Vec<u16>>,
        separators: bool,
        path: impl Into<PathBuf>,
    ) -> CoreResult<()> {
        let path = path.into();
        self.call(|reply| Request::ExportText {
            id,
            pages,
            separators,
            path,
            reply,
        })
    }

    /// Aperçu d'une optimisation (voir `Document::optimize_preview`).
    pub fn optimize_preview(
        &self,
        id: DocId,
        options: crate::optimize::OptimizeOptions,
    ) -> CoreResult<crate::optimize::OptimizeReport> {
        self.call(|reply| Request::OptimizePreview { id, options, reply })
    }

    /// État de sécurité (voir `Document::security`).
    pub fn security(&self, id: DocId) -> CoreResult<crate::security::SecurityInfo> {
        self.call(|reply| Request::Security { id, reply })
    }

    /// Pages qui contiennent déjà du texte (voir `Document::page_has_text`).
    #[cfg(feature = "ocr")]
    pub fn pages_with_text(&self, id: DocId) -> CoreResult<Vec<bool>> {
        self.call(|reply| Request::PagesWithText { id, reply })
    }

    /// Signatures numériques du fichier (voir `Document::signatures`).
    #[cfg(feature = "signature")]
    pub fn signatures(
        &self,
        id: DocId,
    ) -> CoreResult<Vec<crate::digital_signature::SignatureInfo>> {
        self.call(|reply| Request::Signatures { id, reply })
    }

    /// Signe le document avec l'identité du fichier PKCS#12 `p12` et l'enregistre dans
    /// `path` (voir `Document::sign_to`) ; retourne le nouveau résumé et l'état d'édition.
    #[cfg(feature = "signature")]
    pub fn sign_document(
        &self,
        id: DocId,
        path: impl Into<PathBuf>,
        p12: impl Into<PathBuf>,
        password: &str,
        options: crate::digital_signature::SignOptions,
    ) -> CoreResult<(DocumentSummary, EditState)> {
        let (path, p12, password) = (path.into(), p12.into(), password.to_string());
        self.call(|reply| Request::SignDocument {
            id,
            path,
            p12,
            password,
            options,
            reply,
        })
    }

    /// Pièces jointes (voir `Document::attachments`).
    pub fn attachments(&self, id: DocId) -> CoreResult<Vec<crate::attachments::Attachment>> {
        self.call(|reply| Request::Attachments { id, reply })
    }

    /// Extraction d'une pièce jointe (voir `Document::save_attachment`).
    pub fn save_attachment(
        &self,
        id: DocId,
        index: u16,
        path: impl Into<PathBuf>,
    ) -> CoreResult<()> {
        let path = path.into();
        self.call(|reply| Request::SaveAttachment {
            id,
            index,
            path,
            reply,
        })
    }

    /// Comparaison visuelle de deux pages de deux documents ouverts.
    pub fn compare_pages(
        &self,
        id_a: DocId,
        page_a: u16,
        id_b: DocId,
        page_b: u16,
        scale: f32,
    ) -> CoreResult<ComparedPages> {
        self.call(|reply| Request::ComparePages {
            id_a,
            page_a,
            id_b,
            page_b,
            scale,
            reply,
        })
    }

    /// Les parties qu'une scission produirait, selon `mode` (rien n'est écrit).
    pub fn split_plan(
        &self,
        id: DocId,
        mode: crate::pages::SplitMode,
    ) -> CoreResult<Vec<Vec<u16>>> {
        self.call(|reply| Request::SplitPlan { id, mode, reply })
    }
}

/// Les documents ouverts, avec la trace de ceux que la requête en cours a touchés : si elle
/// panique, ce sont eux qu'on ferme (audit du Lot 13, R4).
///
/// \[Apprentissage\] **`RefCell`** : `get` ne reçoit qu'une référence partagée (`&self`) mais
/// doit noter l'identifiant dans `touched`. `RefCell` permet cette modification « intérieure »
/// en vérifiant les emprunts à l'exécution au lieu de la compilation ; ici un seul emprunt
/// à la fois, toujours bref.
#[derive(Default)]
struct Docs {
    map: HashMap<DocId, Entry>,
    touched: RefCell<Vec<DocId>>,
}

impl Docs {
    fn get(&self, id: &DocId) -> Option<&Entry> {
        self.touched.borrow_mut().push(*id);
        self.map.get(id)
    }

    fn get_mut(&mut self, id: &DocId) -> Option<&mut Entry> {
        self.touched.get_mut().push(*id);
        self.map.get_mut(id)
    }

    fn insert(&mut self, id: DocId, entry: Entry) {
        self.touched.get_mut().push(id);
        self.map.insert(id, entry);
    }

    fn remove(&mut self, id: &DocId) -> Option<Entry> {
        self.map.remove(id)
    }
}

/// L'état privé du thread acteur.
struct Actor {
    docs: Docs,
    next_id: DocId,
    cache: PngCache,
    queue: PriorityQueue<Request>,
}

impl Actor {
    fn new(cache_budget: usize) -> Actor {
        Actor {
            docs: Docs::default(),
            next_id: 0,
            cache: PngCache::new(cache_budget),
            queue: PriorityQueue::default(),
        }
    }

    /// Boucle principale : attend au moins une requête, aspire toutes celles déjà arrivées
    /// dans la file priorisée, puis traite la plus prioritaire. Ainsi une page visible
    /// envoyée après dix vignettes passe quand même devant elles.
    fn run(mut self, rx: Receiver<Request>) {
        while self.step(&rx) {}
    }

    /// Un tour de boucle (extrait de `run` pour être testé sans attente) : attend une requête
    /// si la file est vide, aspire celles déjà arrivées, traite la plus prioritaire. Faux
    /// quand plus aucun handle n'existe (l'acteur s'arrête).
    fn step(&mut self, rx: &Receiver<Request>) -> bool {
        if self.queue.is_empty() {
            match rx.recv() {
                Ok(req) => self.queue.push(req.priority(), req),
                Err(_) => return false, // plus aucun handle : on s'arrête
            }
        }
        while let Ok(req) = rx.try_recv() {
            self.queue.push(req.priority(), req);
        }
        if let Some(req) = self.queue.pop() {
            self.docs.touched.get_mut().clear();
            // \[Apprentissage\] **`catch_unwind`** arrête la remontée d'une panique (un bug,
            // un `[i]` hors limites…) à cette frontière : sans lui, le thread de l'acteur
            // mourrait et toutes les opérations suivantes échoueraient jusqu'au redémarrage.
            // `AssertUnwindSafe` affirme qu'on ne réutilisera pas un état à moitié modifié :
            // c'est vrai parce que les documents touchés sont fermés juste après.
            let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| self.handle(req)));
            if outcome.is_err() {
                self.close_touched_documents();
            }
        }
        true
    }

    /// Après une panique : ferme les documents que la requête a touchés (leur état PDFium est
    /// douteux) et oublie leurs rendus en cache. L'appelant reçoit `CoreError::Aborted`.
    fn close_touched_documents(&mut self) {
        let touched = std::mem::take(self.docs.touched.get_mut());
        for id in touched {
            if self.docs.remove(&id).is_some() {
                self.cache.invalidate_doc(id);
                eprintln!("document {id} fermé après une erreur interne");
            }
        }
    }

    fn handle(&mut self, req: Request) {
        // `let _ = reply.send(..)` : si l'appelant n'attend plus (handle détruit, délai
        // dépassé), la réponse est simplement perdue ; ce n'est pas une erreur de l'acteur.
        match req {
            #[cfg(test)]
            #[allow(clippy::panic)]
            Request::Panic { id, reply } => {
                let _ = self.docs.get_mut(&id);
                drop(reply);
                panic!("panique de test (document {id})");
            }
            Request::OpenPath {
                path,
                password,
                reply,
            } => {
                let _ = reply.send(self.open(Document::open(&path, password.as_deref())));
            }
            Request::OpenBytes {
                bytes,
                password,
                reply,
            } => {
                let _ = reply.send(self.open(Document::from_bytes(bytes, password.as_deref())));
            }
            Request::Close { id, reply } => {
                let result = match self.docs.remove(&id) {
                    Some(_) => {
                        self.cache.invalidate_doc(id);
                        Ok(())
                    }
                    None => Err(CoreError::NotFound { id }),
                };
                let _ = reply.send(result);
            }
            Request::Summary { id, reply } => {
                let _ = reply.send(self.entry(id).and_then(|e| summary_of(id, e)));
            }
            Request::SetGeneration {
                id,
                generation,
                reply,
            } => {
                let result = match self.docs.get_mut(&id) {
                    Some(entry) => {
                        entry.generation = entry.generation.max(generation);
                        Ok(entry.generation)
                    }
                    None => Err(CoreError::NotFound { id }),
                };
                let _ = reply.send(result);
            }
            Request::Render { request, reply } => {
                let _ = reply.send(self.render(request));
            }
            Request::TextBoxes { id, page, reply } => {
                let _ = reply.send(self.entry(id).and_then(|e| e.doc.text_boxes(page)));
            }
            Request::Search {
                id,
                query,
                options,
                reply,
            } => {
                let _ = reply.send(
                    self.entry(id)
                        .and_then(|e| search_all(&e.doc, &query, &options)),
                );
            }
            Request::Annotations { id, page, reply } => {
                let _ = reply.send(self.entry(id).and_then(|e| e.doc.annotations(page)));
            }
            Request::AllAnnotations { id, reply } => {
                let _ = reply.send(self.entry(id).and_then(|e| e.doc.all_annotations()));
            }
            Request::PageObjects { id, page, reply } => {
                let _ = reply.send(self.entry(id).and_then(|e| e.doc.page_objects(page)));
            }
            Request::Outline { id, reply } => {
                let _ = reply.send(self.entry(id).and_then(|e| e.doc.outline()));
            }
            Request::Links { id, page, reply } => {
                let _ = reply.send(self.entry(id).and_then(|e| e.doc.links(page)));
            }
            Request::Properties { id, reply } => {
                let _ = reply.send(self.entry(id).and_then(|e| e.doc.properties()));
            }
            Request::Apply { id, command, reply } => {
                let _ = reply.send(self.edit(id, |doc| doc.apply(&command)));
            }
            Request::Undo { id, reply } => {
                let _ = reply.send(self.edit(id, |doc| doc.undo()));
            }
            Request::Redo { id, reply } => {
                let _ = reply.send(self.edit(id, |doc| doc.redo()));
            }
            Request::EditState { id, reply } => {
                let _ = reply.send(self.entry(id).map(|e| e.doc.edit_state()));
            }
            Request::BeginOcrGroup { id, pages, reply } => {
                let result = match self.docs.get_mut(&id) {
                    Some(entry) => {
                        entry
                            .doc
                            .begin_edit_group(crate::edit::ocr_group_label(pages));
                        Ok(())
                    }
                    None => Err(CoreError::NotFound { id }),
                };
                let _ = reply.send(result);
            }
            Request::EndEditGroup { id, reply } => {
                let result = match self.docs.get_mut(&id) {
                    Some(entry) => {
                        entry.doc.end_edit_group();
                        Ok(entry.doc.edit_state())
                    }
                    None => Err(CoreError::NotFound { id }),
                };
                let _ = reply.send(result);
            }
            Request::Save { id, path, reply } => {
                // Enregistrer ne change ni les pages ni le cache : pas d'invalidation.
                let result = match self.docs.get_mut(&id) {
                    Some(entry) => {
                        let saved = match path {
                            Some(p) => entry.doc.save_as(p),
                            None => entry.doc.save(),
                        };
                        saved.and_then(|_| Ok((summary_of(id, entry)?, entry.doc.edit_state())))
                    }
                    None => Err(CoreError::NotFound { id }),
                };
                let _ = reply.send(result);
            }
            Request::ExtractPages {
                id,
                pages,
                path,
                reply,
            } => {
                let result = self
                    .entry(id)
                    .and_then(|e| e.doc.extract_pages(&pages))
                    .and_then(|bytes| crate::edit::write_atomically(&path, &bytes));
                let _ = reply.send(result);
            }
            Request::MergeFiles { paths, reply } => {
                let _ = reply.send(self.merge_files(&paths));
            }
            Request::SplitDocument {
                id,
                parts,
                out_dir,
                base_name,
                reply,
            } => {
                let result = self.entry(id).and_then(|e| {
                    crate::pages::split_to_dir(&e.doc, &parts, &out_dir, &base_name, false)
                });
                let _ = reply.send(result);
            }
            Request::FormFields { id, page, reply } => {
                let _ = reply.send(self.entry(id).and_then(|e| e.doc.form_fields(page)));
            }
            Request::AllFormFields { id, reply } => {
                let _ = reply.send(self.entry(id).and_then(|e| e.doc.all_form_fields()));
            }
            Request::FormInfo { id, reply } => {
                let _ = reply.send(self.entry(id).and_then(|e| e.doc.form_info()));
            }
            Request::PageText { id, page, reply } => {
                let _ = reply.send(self.entry(id).and_then(|e| e.doc.page_text(page)));
            }
            Request::ExportImages {
                id,
                spec,
                dir,
                prefix,
                force,
                reply,
            } => {
                let _ = reply.send(
                    self.entry(id)
                        .and_then(|e| e.doc.export_images(&spec, &dir, &prefix, force)),
                );
            }
            Request::ExportText {
                id,
                pages,
                separators,
                path,
                reply,
            } => {
                let result = self.entry(id).and_then(|e| {
                    let text = e.doc.export_text(pages.as_deref(), separators)?;
                    crate::edit::write_atomically(&path, text.as_bytes())
                });
                let _ = reply.send(result);
            }
            Request::OptimizePreview { id, options, reply } => {
                let _ = reply.send(
                    self.entry(id)
                        .and_then(|e| e.doc.optimize_preview(&options)),
                );
            }
            Request::Security { id, reply } => {
                let _ = reply.send(self.entry(id).and_then(|e| e.doc.security()));
            }
            Request::Attachments { id, reply } => {
                let _ = reply.send(self.entry(id).and_then(|e| e.doc.attachments()));
            }
            #[cfg(feature = "ocr")]
            Request::PagesWithText { id, reply } => {
                let _ = reply.send(self.entry(id).and_then(|e| {
                    (0..e.doc.page_count())
                        .map(|p| e.doc.page_has_text(p))
                        .collect()
                }));
            }
            #[cfg(feature = "signature")]
            Request::Signatures { id, reply } => {
                let _ = reply.send(self.entry(id).and_then(|e| e.doc.signatures()));
            }
            #[cfg(feature = "signature")]
            Request::SignDocument {
                id,
                path,
                p12,
                password,
                options,
                reply,
            } => {
                // Une signature visible dessine un tampon : les rendus en cache sont périmés.
                if options.appearance.is_some() {
                    self.cache.invalidate_doc(id);
                }
                let result = match self.docs.get_mut(&id) {
                    Some(entry) => std::fs::read(&p12)
                        .map_err(CoreError::from)
                        .and_then(|bytes| {
                            crate::digital_signature::SigningIdentity::from_pkcs12(
                                &bytes, &password,
                            )
                        })
                        .and_then(|identity| entry.doc.sign_to(&path, &identity, &options))
                        .and_then(|_| Ok((summary_of(id, entry)?, entry.doc.edit_state()))),
                    None => Err(CoreError::NotFound { id }),
                };
                let _ = reply.send(result);
            }
            Request::SaveAttachment {
                id,
                index,
                path,
                reply,
            } => {
                let _ = reply.send(
                    self.entry(id)
                        .and_then(|e| e.doc.save_attachment(index, &path)),
                );
            }
            Request::ComparePages {
                id_a,
                page_a,
                id_b,
                page_b,
                scale,
                reply,
            } => {
                let result = self.entry(id_a).and_then(|a| {
                    let b = self.entry(id_b)?;
                    let diff =
                        crate::compare::compare_pages(&a.doc, page_a, &b.doc, page_b, scale)?;
                    Ok(ComparedPages {
                        png: diff.image.to_png()?,
                        differing_pixels: diff.differing_pixels,
                        total_pixels: diff.total_pixels,
                        width_px: diff.image.width_px,
                        height_px: diff.image.height_px,
                    })
                });
                let _ = reply.send(result);
            }
            Request::SplitPlan { id, mode, reply } => {
                let result = self
                    .entry(id)
                    .and_then(|e| crate::pages::plan_split(&e.doc, &mode));
                let _ = reply.send(result);
            }
        }
    }

    fn entry(&self, id: DocId) -> CoreResult<&Entry> {
        self.docs.get(&id).ok_or(CoreError::NotFound { id })
    }

    fn open(&mut self, opened: CoreResult<Document>) -> CoreResult<DocumentSummary> {
        let doc = opened?;
        self.next_id += 1;
        let id = self.next_id;
        let entry = Entry { doc, generation: 0 };
        let summary = summary_of(id, &entry)?;
        self.docs.insert(id, entry);
        Ok(summary)
    }

    fn render(&mut self, req: RenderRequest) -> CoreResult<Arc<Vec<u8>>> {
        let entry = self
            .docs
            .get_mut(&req.doc)
            .ok_or(CoreError::NotFound { id: req.doc })?;
        // Le frontend est l'autorité sur la génération : plus ancienne → annulée ;
        // plus récente que ce que l'acteur connaît → on se met à jour.
        if req.generation < entry.generation {
            return Err(CoreError::Cancelled);
        }
        entry.generation = req.generation;
        let entry = &*entry;
        let key = PageKey::new(req.doc, req.page, req.scale, req.rotation);
        if let Some(png) = self.cache.get(&key) {
            return Ok(png);
        }
        let png = Arc::new(
            entry
                .doc
                .render_page_with(req.page, req.scale, req.rotation)?
                .to_png()?,
        );
        self.cache.insert(key, Arc::clone(&png));
        Ok(png)
    }
}

impl Actor {
    /// Applique une modification (`f`) au document `id`, invalide ses rendus en cache et
    /// retourne le résumé et l'état d'édition à jour.
    fn edit(
        &mut self,
        id: DocId,
        f: impl FnOnce(&mut Document) -> CoreResult<()>,
    ) -> CoreResult<(DocumentSummary, EditState)> {
        let entry = self.docs.get_mut(&id).ok_or(CoreError::NotFound { id })?;
        f(&mut entry.doc)?;
        self.cache.invalidate_doc(id);
        let entry = self.docs.get(&id).ok_or(CoreError::NotFound { id })?;
        Ok((summary_of(id, entry)?, entry.doc.edit_state()))
    }

    /// Ouvre chaque fichier, fusionne, et enregistre le résultat comme nouveau document.
    fn merge_files(&mut self, paths: &[PathBuf]) -> CoreResult<DocumentSummary> {
        let docs = paths
            .iter()
            .map(|p| Document::open(p, None))
            .collect::<CoreResult<Vec<_>>>()?;
        let refs: Vec<&Document> = docs.iter().collect();
        let bytes = crate::pages::merge(&refs)?;
        let summary = self.open(Document::from_bytes(bytes, None))?;
        // Le résultat n'existe pas sur disque : « modifié » jusqu'à « Enregistrer sous ».
        if let Some(entry) = self.docs.get_mut(&summary.id) {
            entry.doc.mark_never_saved();
        }
        Ok(summary)
    }
}

fn summary_of(id: DocId, entry: &Entry) -> CoreResult<DocumentSummary> {
    Ok(DocumentSummary {
        id,
        path: entry.doc.path().map(|p| p.display().to_string()),
        title: entry.doc.title(),
        page_count: entry.doc.page_count(),
        pages: entry.doc.page_sizes()?,
        generation: entry.generation,
    })
}

fn search_all(doc: &Document, query: &str, options: &SearchOptions) -> CoreResult<Vec<SearchHit>> {
    let mut hits = Vec::new();
    for page in 0..doc.page_count() {
        hits.extend(doc.search_page(page, query, options)?);
    }
    Ok(hits)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// R4 (audit du Lot 13) : une panique pendant une requête ne tue pas l'acteur. Le
    /// document touché est fermé par précaution (son état PDFium est douteux), l'appelant
    /// reçoit une erreur claire, et les autres documents restent ouverts.
    #[test]
    fn a_panic_closes_only_the_document_it_touched() {
        let service = PdfService::spawn();
        let open = |title: &str| {
            service
                .open_bytes(crate::testing::sample_pdf(1, title), None)
                .unwrap()
                .id
        };
        let (victim, other) = (open("victime"), open("autre"));
        let outcome = service.call(|reply| Request::Panic { id: victim, reply });
        assert!(matches!(outcome, Err(CoreError::Aborted)), "{outcome:?}");
        assert!(matches!(
            service.summary(victim),
            Err(CoreError::NotFound { .. })
        ));
        assert!(service.summary(other).is_ok(), "l'acteur vit toujours");
    }

    /// A17 : ordre de service déterministe (sans `sleep`) : une vignette puis une page
    /// visible arrivent pendant que l'acteur est occupé ; au tour suivant, la visible passe
    /// d'abord.
    #[test]
    fn queued_visible_render_is_served_before_an_older_thumbnail() {
        let (tx, rx) = mpsc::channel::<Request>();
        let mut actor = Actor::new(DEFAULT_CACHE_BUDGET);
        let (open_tx, open_rx) = mpsc::sync_channel(1);
        tx.send(Request::OpenBytes {
            bytes: crate::testing::sample_pdf(3, "file"),
            password: None,
            reply: open_tx,
        })
        .unwrap();
        assert!(actor.step(&rx));
        let id = open_rx.recv().unwrap().unwrap().id;
        let render = |page: u16, priority: Priority| {
            let (reply, done) = mpsc::sync_channel(1);
            let request = RenderRequest {
                doc: id,
                page,
                scale: 0.3,
                rotation: crate::geometry::Rotation::None,
                generation: 0,
                priority,
            };
            tx.send(Request::Render { request, reply }).unwrap();
            done
        };
        let thumbnail = render(1, Priority::Thumbnail);
        let visible = render(2, Priority::Visible);
        assert!(actor.step(&rx));
        assert!(visible.try_recv().is_ok(), "la page visible d'abord");
        assert!(thumbnail.try_recv().is_err(), "la vignette attend");
        assert!(actor.step(&rx));
        assert!(thumbnail.try_recv().is_ok());
    }
}
