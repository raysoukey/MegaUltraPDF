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

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError};

use serde::{Deserialize, Serialize};

pub use cache::DocId;
pub use queue::Priority;

use crate::document::{Document, PageSize};
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
    Links {
        id: DocId,
        page: u16,
        reply: SyncSender<CoreResult<Vec<PageLink>>>,
    },
    Properties {
        id: DocId,
        reply: SyncSender<CoreResult<DocumentProperties>>,
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

// Les requêtes ne sont comparées que par la file de priorité, via leur numéro de séquence :
// l'égalité structurelle n'a pas de sens ici, mais `BinaryHeap` exige `Eq`.
impl PartialEq for Request {
    fn eq(&self, _: &Self) -> bool {
        false
    }
}
impl Eq for Request {}

/// Un document tel que l'acteur le garde.
struct Entry {
    doc: Document,
    generation: u64,
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
        reply_rx.recv().map_err(|_| CoreError::ServiceStopped)?
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

    /// Liens d'une page.
    pub fn links(&self, id: DocId, page: u16) -> CoreResult<Vec<PageLink>> {
        self.call(|reply| Request::Links { id, page, reply })
    }

    /// Propriétés du document.
    pub fn properties(&self, id: DocId) -> CoreResult<DocumentProperties> {
        self.call(|reply| Request::Properties { id, reply })
    }
}

/// L'état privé du thread acteur.
struct Actor {
    docs: HashMap<DocId, Entry>,
    next_id: DocId,
    cache: PngCache,
    queue: PriorityQueue<Request>,
}

impl Actor {
    fn new(cache_budget: usize) -> Actor {
        Actor {
            docs: HashMap::new(),
            next_id: 0,
            cache: PngCache::new(cache_budget),
            queue: PriorityQueue::default(),
        }
    }

    /// Boucle principale : attend au moins une requête, aspire toutes celles déjà arrivées
    /// dans la file priorisée, puis traite la plus prioritaire. Ainsi une page visible
    /// envoyée après dix vignettes passe quand même devant elles.
    fn run(mut self, rx: Receiver<Request>) {
        loop {
            if self.queue.is_empty() {
                match rx.recv() {
                    Ok(req) => self.queue.push(req.priority(), req),
                    Err(_) => return, // plus aucun handle : on s'arrête
                }
            }
            loop {
                match rx.try_recv() {
                    Ok(req) => self.queue.push(req.priority(), req),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => break,
                }
            }
            if let Some(req) = self.queue.pop() {
                self.handle(req);
            }
        }
    }

    fn handle(&mut self, req: Request) {
        // `let _ = reply.send(..)` : si l'appelant n'attend plus (handle détruit, délai
        // dépassé), la réponse est simplement perdue ; ce n'est pas une erreur de l'acteur.
        match req {
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
            Request::Outline { id, reply } => {
                let _ = reply.send(self.entry(id).and_then(|e| e.doc.outline()));
            }
            Request::Links { id, page, reply } => {
                let _ = reply.send(self.entry(id).and_then(|e| e.doc.links(page)));
            }
            Request::Properties { id, reply } => {
                let _ = reply.send(self.entry(id).and_then(|e| e.doc.properties()));
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
