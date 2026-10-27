// SPDX-License-Identifier: GPL-3.0-or-later
//! Commandes d'édition, historique annuler / rétablir, enregistrement atomique.
//!
//! \[Apprentissage\] **Patron Command** : chaque édition est une valeur qui sait se décrire
//! (`label`) et s'exécuter (`apply`). L'historique ne connaît que le trait [`Command`],
//! jamais les commandes concrètes : les lots suivants (annotations, formulaires, contenu)
//! ajouteront des variantes sans toucher à l'historique.
//!
//! **Annuler par memento** : avant d'appliquer une commande, on conserve les octets du
//! document (`to_bytes`). Annuler = recharger ces octets. C'est toujours exact, même pour
//! des opérations sans inverse simple (déplacer des pages, aplatir un formulaire…), au prix
//! d'une copie du document par étape ; l'historique est donc borné en étapes et en octets.

use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::annotations::{AnnotationPatch, NewAnnotation};
use crate::document::{Document, PageSize};
use crate::engine;
use crate::error::{CoreError, CoreResult};
use crate::forms::{FieldValue, SignatureSource};
use crate::geometry::Rect;

/// Une édition de la structure du document, sérialisable (le frontend l'envoie telle quelle).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EditCommand {
    /// Ajoute des quarts de tour (horaires si positifs) à la rotation enregistrée des pages.
    RotatePages {
        /// Pages concernées (index 0-based).
        pages: Vec<u16>,
        /// Nombre de quarts de tour.
        quarter_turns: i32,
    },
    /// Supprime des pages (jamais toutes).
    DeletePages {
        /// Pages à supprimer.
        pages: Vec<u16>,
    },
    /// Déplace des pages pour qu'elles commencent à `to` (index compté une fois retirées).
    MovePages {
        /// Pages à déplacer.
        pages: Vec<u16>,
        /// Index d'insertion.
        to: u16,
    },
    /// Insère une page vide.
    InsertBlankPage {
        /// Index d'insertion (`page_count` = à la fin).
        at: u16,
        /// Dimensions en points.
        size: PageSize,
    },
    /// Insère des pages lues dans un autre fichier.
    InsertPagesFromFile {
        /// Chemin du fichier source.
        path: String,
        /// Pages à prendre (toutes si `None`).
        pages: Option<Vec<u16>>,
        /// Index d'insertion.
        at: u16,
    },
    /// Ajoute une annotation (Lot 3).
    AddAnnotation {
        /// Page (index 0-based).
        page: u16,
        /// Ce qu'il faut créer.
        annotation: NewAnnotation,
    },
    /// Supprime une annotation.
    DeleteAnnotation {
        /// Page.
        page: u16,
        /// Index dans le tableau `/Annots` de la page.
        index: u16,
    },
    /// Modifie une annotation (une note sur place ; les autres sont recréées, leur index
    /// peut changer).
    UpdateAnnotation {
        /// Page.
        page: u16,
        /// Index actuel.
        index: u16,
        /// Les champs à changer.
        patch: AnnotationPatch,
    },
    /// Déplace ou redimensionne une annotation.
    MoveAnnotation {
        /// Page.
        page: u16,
        /// Index actuel.
        index: u16,
        /// Nouvelle emprise (`/Rect`).
        rect: Rect,
    },
    /// Écrit la valeur d'un champ de formulaire (Lot 4).
    SetFieldValue {
        /// Page.
        page: u16,
        /// Index du widget dans `/Annots`.
        index: u16,
        /// La valeur.
        value: FieldValue,
    },
    /// Aplatit le formulaire et les annotations de toutes les pages.
    FlattenForm,
    /// Appose une signature visuelle (contenu de page).
    InsertSignature {
        /// Page.
        page: u16,
        /// Zone, en points PDF.
        rect: Rect,
        /// Image ou traits.
        source: SignatureSource,
    },
    /// Supprime un objet du contenu d'une page (Lot 5).
    DeleteObject {
        /// Page.
        page: u16,
        /// Position de l'objet dans le flux de contenu.
        index: u16,
    },
    /// Déplace un objet du contenu, en axes affichés.
    MoveObject {
        /// Page.
        page: u16,
        /// Position de l'objet.
        index: u16,
        /// Déplacement horizontal (points).
        dx: f32,
        /// Déplacement vertical (points, positif vers le haut).
        dy: f32,
    },
    /// Redimensionne un objet du contenu pour qu'il occupe `rect` (points, espace affiché).
    ResizeObject {
        /// Page.
        page: u16,
        /// Position de l'objet.
        index: u16,
        /// Nouvelle emprise.
        rect: Rect,
    },
    /// Remplace le texte d'un objet texte.
    SetObjectText {
        /// Page.
        page: u16,
        /// Position de l'objet.
        index: u16,
        /// Nouveau texte.
        text: String,
    },
    /// Insère un bloc de texte dans le contenu d'une page.
    InsertText {
        /// Page.
        page: u16,
        /// Coin haut-gauche du bloc (points, espace affiché).
        at: [f32; 2],
        /// Le texte (une ligne par `\n`).
        text: String,
        /// Police, taille, couleur.
        style: crate::content::TextStyle,
    },
    /// Insère une image dans le contenu d'une page.
    InsertImage {
        /// Page.
        page: u16,
        /// Zone d'accueil (l'image y est ajustée et centrée).
        rect: Rect,
        /// Chemin du fichier PNG ou JPEG.
        path: String,
    },
    /// Caviarde une zone : retire textes, images et annotations qui la touchent, puis la
    /// recouvre de noir.
    Redact {
        /// Page.
        page: u16,
        /// Zone (points, espace affiché).
        rect: Rect,
    },
    /// Pose un filigrane.
    Watermark {
        /// Texte, style, pages.
        spec: crate::overlay::Watermark,
    },
    /// Numérote les pages (en-tête ou pied à base de modèle).
    PageNumbers {
        /// Modèle, position, pages.
        spec: crate::overlay::PageNumbering,
    },
    /// Recadre des pages (`/CropBox`).
    CropPages {
        /// Marges et pages.
        spec: crate::overlay::CropSpec,
    },
    /// Modifie les métadonnées du document (Lot 6).
    SetMetadata {
        /// Champs à changer.
        patch: crate::properties::MetadataPatch,
    },
    /// Protège le document par mot de passe (AES-256).
    Protect {
        /// Mots de passe et permissions.
        protection: crate::security::Protection,
    },
    /// Retire la protection : avec le mot de passe saisi, sinon celui de l'ouverture.
    Unprotect {
        /// Mot de passe saisi (utilisateur ou propriétaire) ; `None` = celui de l'ouverture.
        #[serde(default)]
        password: Option<String>,
    },
    /// Optimise le document (élagage, compression, images).
    Optimize {
        /// Réglages.
        options: crate::optimize::OptimizeOptions,
    },
    /// Joint un fichier au document.
    AddAttachment {
        /// Chemin du fichier à joindre.
        path: String,
    },
    /// Retire une pièce jointe.
    DeleteAttachment {
        /// Position dans la liste des pièces jointes.
        index: u16,
    },
    /// Reconnaît le texte d'une page numérisée (Tesseract) et l'ajoute en couche invisible.
    #[cfg(feature = "ocr")]
    OcrPage {
        /// Index de la page (à partir de 0).
        page: u16,
        /// Langues Tesseract (`eng`, `fra`…), dans l'ordre de préférence.
        languages: Vec<String>,
    },
}

/// Ce qu'une édition doit savoir faire pour entrer dans l'historique.
pub trait Command {
    /// Libellé court pour l'interface (« Supprimer 2 pages »).
    fn label(&self) -> String;
    /// Applique l'édition ; en cas d'erreur, le document doit rester inchangé.
    fn apply(&self, doc: &mut Document) -> CoreResult<()>;
    /// Libellé précisé d'après l'état du document **avant** l'application (le genre de
    /// l'annotation supprimée, le nom du champ rempli). Par défaut : [`label`](Self::label).
    fn label_in(&self, _doc: &Document) -> String {
        self.label()
    }
}

fn plural(n: usize, singular: &str, plural: &str) -> String {
    if n == 1 {
        format!("1 {singular}")
    } else {
        format!("{n} {plural}")
    }
}

impl Command for EditCommand {
    fn label(&self) -> String {
        match self {
            EditCommand::RotatePages { pages, .. } => {
                format!("Pivoter {}", plural(pages.len(), "page", "pages"))
            }
            EditCommand::DeletePages { pages } => {
                format!("Supprimer {}", plural(pages.len(), "page", "pages"))
            }
            EditCommand::MovePages { pages, .. } => {
                format!("Déplacer {}", plural(pages.len(), "page", "pages"))
            }
            EditCommand::InsertBlankPage { .. } => "Insérer une page vide".to_string(),
            EditCommand::InsertPagesFromFile { path, pages, .. } => {
                let name = Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.clone());
                match pages {
                    Some(p) => {
                        format!("Insérer {} depuis {name}", plural(p.len(), "page", "pages"))
                    }
                    None => format!("Insérer les pages de {name}"),
                }
            }
            EditCommand::AddAnnotation { annotation, .. } => {
                format!("Ajouter : {}", annotation.kind.label_fr())
            }
            // Le genre n'est connu qu'à l'exécution ; le libellé est précisé par `apply`
            // via `label_with_kind` (voir `Document::apply` pour les annotations).
            EditCommand::DeleteAnnotation { .. } => "Supprimer : Annotation".to_string(),
            EditCommand::UpdateAnnotation { .. } => "Modifier : Annotation".to_string(),
            EditCommand::MoveAnnotation { .. } => "Déplacer : Annotation".to_string(),
            EditCommand::SetFieldValue { .. } => "Remplir un champ".to_string(),
            EditCommand::FlattenForm => "Aplatir le formulaire".to_string(),
            EditCommand::InsertSignature { .. } => "Apposer une signature".to_string(),
            EditCommand::DeleteObject { .. } => "Supprimer : Objet".to_string(),
            EditCommand::MoveObject { .. } => "Déplacer : Objet".to_string(),
            EditCommand::ResizeObject { .. } => "Redimensionner : Objet".to_string(),
            EditCommand::SetObjectText { .. } => "Modifier le texte".to_string(),
            EditCommand::InsertText { .. } => "Insérer du texte".to_string(),
            EditCommand::InsertImage { .. } => "Insérer une image".to_string(),
            EditCommand::Redact { .. } => "Caviarder".to_string(),
            EditCommand::Watermark { .. } => "Filigraner".to_string(),
            EditCommand::PageNumbers { .. } => "Numéroter".to_string(),
            EditCommand::CropPages { .. } => "Recadrer".to_string(),
            EditCommand::SetMetadata { .. } => "Modifier les métadonnées".to_string(),
            EditCommand::Protect { .. } => "Protéger le document".to_string(),
            EditCommand::Unprotect { .. } => "Retirer la protection".to_string(),
            EditCommand::Optimize { .. } => "Optimiser".to_string(),
            EditCommand::AddAttachment { path } => format!(
                "Joindre : {}",
                Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.clone())
            ),
            EditCommand::DeleteAttachment { .. } => "Retirer la pièce jointe".to_string(),
            #[cfg(feature = "ocr")]
            EditCommand::OcrPage { page, .. } => {
                format!("Reconnaître le texte (page {})", u32::from(*page) + 1)
            }
        }
    }

    fn label_in(&self, doc: &Document) -> String {
        let annotation = |verb: &str, page: u16, index: u16| {
            doc.annotations(page)
                .ok()
                .and_then(|a| a.into_iter().find(|a| a.index == index))
                .map(|a| format!("{verb} : {}", a.kind.label_fr()))
        };
        let object = |verb: &str, page: u16, index: u16| {
            doc.page_objects(page)
                .ok()
                .and_then(|o| o.into_iter().find(|o| o.index == index))
                .map(|o| format!("{verb} : {}", o.kind.label_fr()))
        };
        let label = match self {
            EditCommand::DeleteAnnotation { page, index } => annotation("Supprimer", *page, *index),
            EditCommand::MoveAnnotation { page, index, .. } => {
                annotation("Déplacer", *page, *index)
            }
            EditCommand::UpdateAnnotation { page, index, .. } => {
                annotation("Modifier", *page, *index)
            }
            EditCommand::SetFieldValue { page, index, .. } => doc
                .form_fields(*page)
                .ok()
                .and_then(|f| f.into_iter().find(|f| f.index == *index))
                .map(|f| format!("Remplir : {}", f.name)),
            EditCommand::DeleteObject { page, index } => object("Supprimer", *page, *index),
            EditCommand::MoveObject { page, index, .. } => object("Déplacer", *page, *index),
            EditCommand::ResizeObject { page, index, .. } => {
                object("Redimensionner", *page, *index)
            }
            EditCommand::Watermark { spec } => Some(pages_label("Filigraner", &spec.pages, doc)),
            EditCommand::PageNumbers { spec } => Some(pages_label("Numéroter", &spec.pages, doc)),
            EditCommand::CropPages { spec } => Some(pages_label("Recadrer", &spec.pages, doc)),
            EditCommand::DeleteAttachment { index } => doc
                .attachments()
                .ok()
                .and_then(|a| a.into_iter().find(|a| a.index == *index))
                .map(|a| format!("Retirer la pièce jointe : {}", a.name)),
            _ => None,
        };
        label.unwrap_or_else(|| self.label())
    }

    fn apply(&self, doc: &mut Document) -> CoreResult<()> {
        match self {
            EditCommand::RotatePages {
                pages,
                quarter_turns,
            } => doc.rotate_pages(pages, *quarter_turns),
            EditCommand::DeletePages { pages } => doc.delete_pages(pages),
            EditCommand::MovePages { pages, to } => doc.move_pages(pages, *to),
            EditCommand::InsertBlankPage { at, size } => doc.insert_blank_page(*at, *size),
            EditCommand::InsertPagesFromFile { path, pages, at } => {
                let other = Document::open(path, None)?;
                let all: Vec<u16> = (0..other.page_count()).collect();
                doc.insert_pages_from(&other, pages.as_deref().unwrap_or(&all), *at)
            }
            EditCommand::AddAnnotation { page, annotation } => {
                doc.add_annotation(*page, annotation).map(|_| ())
            }
            EditCommand::DeleteAnnotation { page, index } => doc.delete_annotation(*page, *index),
            EditCommand::UpdateAnnotation { page, index, patch } => {
                doc.update_annotation(*page, *index, patch).map(|_| ())
            }
            EditCommand::MoveAnnotation { page, index, rect } => {
                doc.set_annotation_rect(*page, *index, *rect).map(|_| ())
            }
            EditCommand::SetFieldValue { page, index, value } => {
                doc.set_field_value(*page, *index, value)
            }
            EditCommand::FlattenForm => doc.flatten_all(),
            EditCommand::InsertSignature { page, rect, source } => {
                doc.insert_signature(*page, *rect, source)
            }
            EditCommand::DeleteObject { page, index } => doc.delete_object(*page, *index),
            EditCommand::MoveObject {
                page,
                index,
                dx,
                dy,
            } => doc.move_object(*page, *index, *dx, *dy),
            EditCommand::ResizeObject { page, index, rect } => {
                doc.resize_object(*page, *index, *rect)
            }
            EditCommand::SetObjectText { page, index, text } => {
                doc.set_object_text(*page, *index, text)
            }
            EditCommand::InsertText {
                page,
                at,
                text,
                style,
            } => doc.insert_text(*page, *at, text, style),
            EditCommand::InsertImage { page, rect, path } => doc.insert_image(*page, *rect, path),
            EditCommand::Redact { page, rect } => doc.redact(*page, *rect).map(|_| ()),
            EditCommand::Watermark { spec } => doc.add_watermark(spec),
            EditCommand::PageNumbers { spec } => doc.add_page_numbers(spec),
            EditCommand::CropPages { spec } => doc.crop_pages(spec),
            EditCommand::SetMetadata { patch } => doc.set_metadata(patch),
            EditCommand::Protect { protection } => doc.protect(protection),
            EditCommand::Unprotect { password } => doc.unprotect_with(password.as_deref()),
            EditCommand::Optimize { options } => {
                // Sans gain, rien ne change : on refuse pour ne pas empiler une étape vide
                // dans l'historique ni marquer le document « modifié ».
                let report = doc.optimize(options)?;
                if report.bytes_after >= report.bytes_before {
                    return Err(CoreError::InvalidOperation(
                        "le document est déjà optimisé : rien à gagner".to_string(),
                    ));
                }
                Ok(())
            }
            EditCommand::AddAttachment { path } => doc.add_attachment(Path::new(path)),
            EditCommand::DeleteAttachment { index } => doc.delete_attachment(*index),
            #[cfg(feature = "ocr")]
            EditCommand::OcrPage { page, languages } => doc.ocr_page(*page, languages),
        }
    }
}

/// « Verbe N page(s) » pour une commande qui vise une liste de pages (toutes si `None`).
fn pages_label(verb: &str, pages: &Option<Vec<u16>>, doc: &Document) -> String {
    let n = match pages {
        Some(list) => list.iter().collect::<std::collections::BTreeSet<_>>().len(),
        None => usize::from(doc.page_count()),
    };
    format!("{verb} {}", plural(n, "page", "pages"))
}

/// Bornes de l'historique.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryLimits {
    /// Nombre maximal d'étapes annulables.
    pub max_steps: usize,
    /// Mémoire maximale des instantanés annulables (octets) ; le plus récent est toujours gardé.
    pub max_bytes: usize,
}

impl Default for HistoryLimits {
    fn default() -> Self {
        HistoryLimits {
            max_steps: 20,
            max_bytes: 256 * 1024 * 1024,
        }
    }
}

/// Un instantané : le libellé de la commande qui l'a produit et le document **avant** elle
/// (pile annuler) ou **après** elle (pile rétablir), avec le mot de passe qui permet de
/// relire ces octets (une protection posée ou retirée change ce mot de passe).
struct Snapshot {
    label: String,
    bytes: Vec<u8>,
    password: Option<String>,
}

/// Historique d'un document.
///
/// `position` compte les éditions appliquées depuis l'ouverture le long de la ligne du
/// temps courante ; `saved_position` est la position au dernier enregistrement (`None` si
/// l'état enregistré n'est plus atteignable, après une édition qui suit un annuler).
pub(crate) struct History {
    limits: HistoryLimits,
    undo: VecDeque<Snapshot>,
    redo: Vec<Snapshot>,
    position: u64,
    saved_position: Option<u64>,
}

impl History {
    pub(crate) fn new(limits: HistoryLimits) -> History {
        History {
            limits,
            undo: VecDeque::new(),
            redo: Vec::new(),
            position: 0,
            saved_position: Some(0),
        }
    }

    fn push_undo(&mut self, snapshot: Snapshot) {
        self.undo.push_back(snapshot);
        while self.undo.len() > self.limits.max_steps.max(1) {
            self.undo.pop_front();
        }
        let mut used: usize = self.undo.iter().map(|s| s.bytes.len()).sum();
        while used > self.limits.max_bytes && self.undo.len() > 1 {
            if let Some(old) = self.undo.pop_front() {
                used -= old.bytes.len();
            }
        }
    }
}

/// Résumé de l'historique pour l'interface.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct EditState {
    /// Le document diffère de l'état enregistré.
    pub modified: bool,
    /// Une étape peut être annulée.
    pub can_undo: bool,
    /// Une étape peut être rétablie.
    pub can_redo: bool,
    /// Libellé de l'étape à annuler.
    pub undo_label: Option<String>,
    /// Libellé de l'étape à rétablir.
    pub redo_label: Option<String>,
}

impl Document {
    /// Remplace les bornes de l'historique (surtout pour les tests).
    pub fn with_history_limits(mut self, limits: HistoryLimits) -> Document {
        self.history = History::new(limits);
        self
    }

    /// Applique une commande et l'enregistre dans l'historique. Si la commande échoue,
    /// ni le document ni l'historique ne changent.
    pub fn apply(&mut self, command: &dyn Command) -> CoreResult<()> {
        let label = self.label_for(command);
        let before = self.to_bytes()?;
        let password_before = self.password().map(str::to_string);
        let changed_before = self.changed;
        if let Err(e) = command.apply(self) {
            // Une commande peut échouer après avoir commencé à modifier le document
            // (plusieurs appels PDFium) : on revient à l'instantané pris juste avant,
            // ce qui rend le contrat « inchangé en cas d'erreur » vrai pour toute commande.
            // Le drapeau `changed` aussi (posé en tête par la méthode refusée) : sinon
            // l'enregistrement suivant réécrirait un fichier intact, signatures comprises.
            self.set_password(password_before);
            self.reload_from_bytes(before)?;
            self.changed = changed_before;
            return Err(e);
        }
        if !self.history.redo.is_empty() {
            self.history.redo.clear();
            // L'état enregistré appartenait peut-être à la branche abandonnée.
            if self.history.saved_position > Some(self.history.position) {
                self.history.saved_position = None;
            }
        }
        self.history.push_undo(Snapshot {
            label,
            bytes: before,
            password: password_before,
        });
        self.history.position += 1;
        // Le document n'est plus celui du fichier : le prochain enregistrement le réécrit.
        self.changed = true;
        Ok(())
    }

    /// Libellé d'une commande, précisé quand il dépend de l'état du document (le genre de
    /// l'annotation supprimée ou déplacée n'est connu qu'avant l'application).
    fn label_for(&self, command: &dyn Command) -> String {
        command.label_in(self)
    }

    /// Annule la dernière édition.
    pub fn undo(&mut self) -> CoreResult<()> {
        let Some(snapshot) = self.history.undo.pop_back() else {
            return Err(CoreError::InvalidOperation("rien à annuler".to_string()));
        };
        // Si le rechargement échoue, l'étape est remise en place : rien n'est perdu.
        let (label, bytes, password) = match self.swap_in(&snapshot) {
            Ok((current, password)) => (snapshot.label, current, password),
            Err(e) => {
                self.history.undo.push_back(snapshot);
                return Err(e);
            }
        };
        self.history.redo.push(Snapshot {
            label,
            bytes,
            password,
        });
        self.history.position -= 1;
        // De retour à l'état enregistré : de nouveau identique au fichier.
        self.changed = self.is_modified();
        Ok(())
    }

    /// Remplace le document par l'instantané et retourne les octets et le mot de passe de
    /// l'état courant (pour l'autre pile). En cas d'erreur le document n'a pas changé.
    fn swap_in(&mut self, snapshot: &Snapshot) -> CoreResult<(Vec<u8>, Option<String>)> {
        let current = self.to_bytes()?;
        let current_password = self.password().map(str::to_string);
        self.set_password(snapshot.password.clone());
        if let Err(e) = self.reload_from_bytes(snapshot.bytes.clone()) {
            self.set_password(current_password);
            return Err(e);
        }
        Ok((current, current_password))
    }

    /// Rétablit la dernière édition annulée.
    pub fn redo(&mut self) -> CoreResult<()> {
        let Some(snapshot) = self.history.redo.pop() else {
            return Err(CoreError::InvalidOperation("rien à rétablir".to_string()));
        };
        let (label, bytes, password) = match self.swap_in(&snapshot) {
            Ok((current, password)) => (snapshot.label, current, password),
            Err(e) => {
                self.history.redo.push(snapshot);
                return Err(e);
            }
        };
        self.history.push_undo(Snapshot {
            label,
            bytes,
            password,
        });
        self.history.position += 1;
        self.changed = self.is_modified();
        Ok(())
    }

    /// Déclare que ce document n'a jamais été enregistré (résultat d'une fusion) : il est
    /// « modifié » tant qu'un « Enregistrer sous » n'a pas eu lieu, quoi qu'on annule.
    pub fn mark_never_saved(&mut self) {
        self.history.saved_position = None;
        // Ses octets sont ceux de la fusion, jamais nettoyés : « Enregistrer » les réécrit.
        self.changed = true;
    }

    /// Vrai si une étape peut être annulée.
    pub fn can_undo(&self) -> bool {
        !self.history.undo.is_empty()
    }

    /// Vrai si une étape peut être rétablie.
    pub fn can_redo(&self) -> bool {
        !self.history.redo.is_empty()
    }

    /// Libellé de l'étape à annuler.
    pub fn undo_label(&self) -> Option<String> {
        self.history.undo.back().map(|s| s.label.clone())
    }

    /// Libellé de l'étape à rétablir.
    pub fn redo_label(&self) -> Option<String> {
        self.history.redo.last().map(|s| s.label.clone())
    }

    /// Le document diffère-t-il de l'état enregistré (ou ouvert) ?
    pub fn is_modified(&self) -> bool {
        self.history.saved_position != Some(self.history.position)
    }

    /// Résumé pour l'interface.
    pub fn edit_state(&self) -> EditState {
        EditState {
            modified: self.is_modified(),
            can_undo: self.can_undo(),
            can_redo: self.can_redo(),
            undo_label: self.undo_label(),
            redo_label: self.redo_label(),
        }
    }

    /// Enregistre le document à son chemin d'origine (écriture atomique).
    pub fn save(&mut self) -> CoreResult<()> {
        let Some(path) = self.path().map(Path::to_path_buf) else {
            return Err(CoreError::InvalidOperation(
                "ce document n'a pas de fichier : utilisez « Enregistrer sous »".to_string(),
            ));
        };
        self.save_as(path)
    }

    /// Enregistre le document dans `path` et en fait son nouveau chemin.
    ///
    /// \[Apprentissage\] **Écriture atomique** : les octets sont écrits dans un fichier
    /// temporaire du même dossier, puis renommé par-dessus la cible (`persist`, qui remplace
    /// un fichier existant d'un seul coup, y compris sous Windows). À aucun moment la cible
    /// n'existe à moitié écrite ; si l'écriture échoue, le temporaire est supprimé et
    /// l'original est intact.
    pub fn save_as(&mut self, path: impl AsRef<Path>) -> CoreResult<()> {
        let path = path.as_ref();
        // Document intact : les octets du fichier, tels quels (signatures préservées).
        if !self.changed {
            write_atomically(path, &self.file_bytes)?;
            // Les octets du fichier restent les mêmes : on les reprend sans copie.
            let bytes = std::mem::take(&mut self.file_bytes);
            self.mark_saved(path, bytes);
            return Ok(());
        }
        let bytes = self.rewritten_bytes()?;
        write_atomically(path, &bytes)?;
        // Ce qui vient d'être écrit est désormais l'état du fichier.
        self.mark_saved(path, bytes);
        Ok(())
    }

    /// Le document vient d'être écrit dans `path` avec les octets `bytes` (enregistrement ou
    /// signature) : ce fichier devient son état de référence.
    pub(crate) fn mark_saved(&mut self, path: &Path, bytes: Vec<u8>) {
        self.set_path(Some(path.to_path_buf()));
        self.history.saved_position = Some(self.history.position);
        self.file_bytes = bytes;
        self.changed = false;
    }

    /// Le document réécrit tel que « Enregistrer » l'écrirait (sans l'écrire) : PDFium, puis
    /// passe de nettoyage `lopdf`. Sert aussi de base à une signature d'un document modifié.
    pub(crate) fn rewritten_bytes(&mut self) -> CoreResult<Vec<u8>> {
        // Chaque rendu avec annotations laisse dans le document PDFium des objets orphelins
        // (apparences de popup) que `save_to_bytes` écrirait. Recharger depuis les octets
        // courants ne garde que les objets atteignables : le fichier enregistré est propre,
        // et le document en mémoire repart léger.
        let dirty = self.to_bytes()?;
        let bytes = {
            let guard = engine::guard();
            let clean = engine::load_bytes(&guard, dirty, self.password())?;
            let bytes = clean.save_to_bytes()?;
            self.replace_inner(clean);
            bytes
        };
        // Passe `lopdf` : /InkList, apparences imposées aux notes, élagage (module `cleanup`).
        crate::cleanup::before_save(&bytes)
    }

    /// Recharge le document depuis des octets (annuler / rétablir) en gardant son chemin.
    pub(crate) fn reload_from_bytes(&mut self, bytes: Vec<u8>) -> CoreResult<()> {
        let guard = engine::guard();
        let inner = engine::load_bytes(&guard, bytes, self.password())?;
        self.replace_inner(inner);
        Ok(())
    }
}

/// Écrit `bytes` dans `path` de façon atomique : fichier temporaire du même dossier, puis
/// renommage par-dessus la cible. En cas d'échec, la cible n'est pas touchée.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> CoreResult<()> {
    // Un lien symbolique est suivi : on remplace le fichier visé, pas le lien.
    let is_link = std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);
    let target = if is_link {
        std::fs::canonicalize(path)?
    } else {
        path.to_path_buf()
    };
    let dir = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let mut builder = tempfile::Builder::new();
    builder.prefix(".megaultrapdf-").suffix(".tmp");
    // Sous POSIX, le temporaire naît en 0600 ; le fichier remplacé garderait ce mode.
    // On reprend les permissions de la cible existante (fichier partagé, exécutable…).
    #[cfg(unix)]
    if let Ok(meta) = std::fs::metadata(&target) {
        builder.permissions(meta.permissions());
    }
    let mut temp = builder.tempfile_in(&dir)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    // `persist` échoue si la cible est un dossier : le temporaire est alors supprimé
    // (il part avec l'erreur) et la cible n'est pas touchée.
    temp.persist(&target).map_err(|e| CoreError::Io(e.error))?;
    Ok(())
}
