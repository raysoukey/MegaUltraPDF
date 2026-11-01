// SPDX-License-Identifier: GPL-3.0-or-later
//! Un document PDF ouvert : métadonnées, pages, sérialisation.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use pdfium_render::prelude::{PdfDocument, PdfDocumentMetadataTagType, PdfPageIndex};
use serde::{Deserialize, Serialize};

use crate::engine;
use crate::error::{CoreError, CoreResult};

/// Dimensions d'une page en points PDF (1 point = 1/72 de pouce).
///
/// Dérive `Serialize` pour traverser la frontière Rust → TypeScript telle quelle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PageSize {
    /// Largeur en points.
    pub width_pt: f32,
    /// Hauteur en points.
    pub height_pt: f32,
}

/// Un PDF chargé en mémoire, prêt à être lu, rendu et, plus tard, modifié.
///
/// \[Apprentissage\] `Document` **possède** son `PdfDocument` interne. Celui-ci emprunte
/// l'instance PDFium globale (durée de vie `'static`, voir [`engine`]), ce qui évite
/// d'avoir une structure qui se référence elle-même. Quand `Document` est détruit
/// (`Drop`), PDFium libère ses ressources automatiquement.
pub struct Document {
    /// Toujours `Some` pendant la vie du document ; `None` seulement pendant `Drop`,
    /// qui doit libérer le `PdfDocument` en tenant le jeton PDFium (voir [`engine::guard`]).
    inner: Option<PdfDocument<'static>>,
    path: Option<PathBuf>,
    /// Mot de passe d'ouverture, nécessaire pour recharger les instantanés d'un PDF chiffré
    /// (PDFium réenregistre un document chiffré **chiffré**, avec le même mot de passe).
    password: Option<String>,
    /// Historique annuler / rétablir (module `edit`).
    pub(crate) history: crate::edit::History,
    /// Octets exacts du fichier tel qu'il a été chargé ou enregistré. Tant que `changed` est
    /// faux, `save` les écrit tels quels, sans réécriture par PDFium, ce qui garde valides
    /// les signatures numériques (elles couvrent des octets précis) ; leur vérification lit
    /// toujours ces octets.
    pub(crate) file_bytes: Vec<u8>,
    /// Vrai dès qu'une modification éloigne le document de `file_bytes` ; redevient faux à
    /// l'enregistrement, ou quand annuler / rétablir ramène à l'état enregistré.
    pub(crate) changed: bool,
    /// Une opération qui retire une information (annotation, page, objet, texte, pièce jointe,
    /// caviardage) a eu lieu depuis le dernier enregistrement : `rewritten_bytes` refuse alors
    /// d'écrire si le nettoyage `lopdf` n'a pas pu s'appliquer, car les restes de ce qui a été
    /// supprimé survivraient dans le fichier (audit E1).
    ///
    /// Drapeau **prudent** : `ensure_cleanable` le pose avant que l'opération ne valide ses
    /// arguments (une opération refusée ensuite le laisse levé), et annuler ne le baisse pas
    /// (l'instantané rechargé peut lui aussi porter des orphelins). Seul l'enregistrement le
    /// remet à faux.
    pub(crate) needs_cleanup: bool,
    /// Compte rendu du caviardage que la dernière édition vient d'appliquer (sinon `None`) :
    /// l'interface le reçoit avec l'état de l'historique pour avertir l'utilisateur.
    pub(crate) last_redaction: Option<crate::content::redact::RedactReport>,
    /// Police Unicode (DejaVu Sans) déjà chargée dans le `PdfDocument` courant : chaque
    /// chargement incorpore une copie entière de la police, on la réutilise donc tant que
    /// le document PDFium n'est pas remplacé (voir [`Document::replace_inner`]).
    unicode_font: Option<pdfium_render::prelude::PdfFontToken>,
    /// Le document courant lu par `lopdf` (repli de la couche de texte, pièces jointes de
    /// page, propriétés), chargé au premier besoin puis gardé : sans lui, chaque page d'un long
    /// document relirait tout le fichier. `Some(None)` : `lopdf` n'a pas su le lire. Vidé par
    /// toute modification ([`Document::clear_lopdf_cache`]). Privé à ce fichier : on ne le lit
    /// que par [`Document::lopdf_view`].
    ///
    /// \[Apprentissage\] **`OnceLock`** se remplit une seule fois à travers une référence
    /// partagée (`&self`), comme l'instance PDFium globale (`engine.rs`) ; le vider demande
    /// `&mut self` (`take`), ce qui garantit que personne ne lit le cache à ce moment-là.
    lopdf_cache: std::sync::OnceLock<Option<lopdf::Document>>,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("path", &self.path)
            .field("page_count", &self.page_count())
            .finish()
    }
}

impl Drop for Document {
    /// Libère les ressources PDFium sous le jeton, comme toute autre opération.
    fn drop(&mut self) {
        let _guard = engine::guard();
        self.inner.take();
    }
}

impl Document {
    /// Ouvre le fichier `path`. `password` est requis pour un PDF chiffré.
    ///
    /// Le fichier est lu entièrement en mémoire : le document ne garde aucun
    /// descripteur ouvert, on peut donc le réécrire au même endroit plus tard.
    pub fn open(path: impl AsRef<Path>, password: Option<&str>) -> CoreResult<Document> {
        let path = path.as_ref();
        let size = std::fs::metadata(path)?.len();
        if size > crate::error::MAX_FILE_BYTES {
            return Err(CoreError::FileTooLarge { bytes: size });
        }
        let bytes = std::fs::read(path)?;
        let mut doc = Self::from_bytes(bytes, password)?;
        doc.path = Some(path.to_path_buf());
        Ok(doc)
    }

    /// Charge un PDF depuis des octets déjà en mémoire.
    pub fn from_bytes(bytes: Vec<u8>, password: Option<&str>) -> CoreResult<Document> {
        let size = bytes.len() as u64;
        if size > crate::error::MAX_FILE_BYTES {
            return Err(CoreError::FileTooLarge { bytes: size });
        }
        let file_bytes = bytes.clone();
        // Constat F21-5 : PDFium reçoit une copie dont les XMP compressés sont décodés (il en
        // retirerait le `/Filter` en gardant les octets compressés) ; `file_bytes` reste
        // l'original. Le cas courant est écarté en lisant les octets, sans analyse.
        let bytes = crate::crypt::unfiltered_metadata_copy(&bytes).unwrap_or(bytes);
        let guard = engine::guard();
        let inner = match engine::load_bytes(&guard, bytes, password) {
            // Chiffrement par certificat : aucun mot de passe ne l'ouvrira (C9, Lot 13).
            Err(_) if uses_certificate_encryption(&file_bytes) => {
                return Err(CoreError::UnsupportedEncryption(
                    "par certificat (clé d'un destinataire)".to_string(),
                ));
            }
            // PDFium ne distingue pas « pas de mot de passe » de « mauvais mot de passe ».
            Err(CoreError::PasswordRequired) if password.is_some() => {
                return Err(CoreError::WrongPassword);
            }
            other => other?,
        };
        // Invariant du crate : un index de page tient dans un `u16` (voir `count_unlocked`).
        if inner.pages().len() > i32::from(u16::MAX) {
            return Err(CoreError::InvalidPdf(
                "plus de 65 535 pages, limite de MegaUltraPDF".to_string(),
            ));
        }
        Ok(Document {
            inner: Some(inner),
            path: None,
            password: password.map(str::to_string),
            history: crate::edit::History::new(crate::edit::HistoryLimits::default()),
            file_bytes,
            changed: false,
            needs_cleanup: false,
            last_redaction: None,
            unicode_font: None,
            lopdf_cache: std::sync::OnceLock::new(),
        })
    }

    /// Chemin d'origine, s'il a été ouvert depuis un fichier.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Titre déclaré dans les métadonnées, ou `None` s'il est absent ou vide.
    pub fn title(&self) -> Option<String> {
        let _guard = engine::guard();
        self.inner()
            .metadata()
            .get(PdfDocumentMetadataTagType::Title)
            .map(|tag| tag.value().trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Nombre de pages.
    pub fn page_count(&self) -> u16 {
        let _guard = engine::guard();
        self.count_unlocked()
    }

    /// Dimensions de la page `index` (à partir de 0).
    pub fn page_size(&self, index: u16) -> CoreResult<PageSize> {
        let _guard = engine::guard();
        let rect = self
            .inner()
            .pages()
            .page_size(self.checked_index_unlocked(index)?)?;
        Ok(PageSize {
            width_pt: rect.width().value,
            height_pt: rect.height().value,
        })
    }

    /// Repère de la page `index` : sa rotation `/Rotate` et sa zone visible, pour convertir
    /// entre l'espace affiché (celui de toute l'API) et l'espace utilisateur du fichier.
    pub fn page_frame(&self, index: u16) -> CoreResult<crate::geometry::PageFrame> {
        let _guard = engine::guard();
        let page = self
            .inner()
            .pages()
            .get(self.checked_index_unlocked(index)?)?;
        Ok(crate::geometry::frame_of(&page))
    }

    /// Dimensions de toutes les pages, dans l'ordre.
    pub fn page_sizes(&self) -> CoreResult<Vec<PageSize>> {
        (0..self.page_count()).map(|i| self.page_size(i)).collect()
    }

    /// Sérialise le document complet en octets PDF.
    ///
    /// Dans un document chiffré, PDFium écrit le XMP (`/Metadata`) en clair sans le déclarer,
    /// puis le détruirait en relisant sa propre sortie : il est rechiffré ici
    /// (`crypt::seal_metadata`, tâche 21b du Lot 14), là où naissent les
    /// octets PDFium d'un document ouvert (enregistrement, instantanés d'historique ; la
    /// seconde sortie de `rewritten_bytes` passe par la même fonction). Sans
    /// chiffrement, ce contrôle ne lit que la remorque.
    pub fn to_bytes(&self) -> CoreResult<Vec<u8>> {
        // [Apprentissage] Un bloc `{ … }` borne la vie du jeton `_guard` : le verrou PDFium est
        // rendu à la fin du bloc, avant le travail `lopdf` qui n'en a pas besoin.
        let bytes = {
            let _guard = engine::guard();
            self.inner().save_to_bytes()?
        };
        Ok(crate::crypt::seal_metadata(bytes, self.password()))
    }

    /// Nombre de pages, **jeton déjà pris par l'appelant**.
    ///
    /// `from_bytes` refuse tout document de plus de `u16::MAX` pages, donc la conversion
    /// ne peut pas échouer ; le repli sur `u16::MAX` n'est qu'une ceinture de sécurité.
    pub(crate) fn count_unlocked(&self) -> u16 {
        u16::try_from(self.inner().pages().len()).unwrap_or(u16::MAX)
    }

    /// Vérifie la borne et convertit vers le type d'index attendu par PDFium,
    /// **jeton déjà pris par l'appelant**.
    pub(crate) fn checked_index_unlocked(&self, index: u16) -> CoreResult<PdfPageIndex> {
        let count = self.count_unlocked();
        if index >= count {
            return Err(CoreError::PageOutOfRange { index, count });
        }
        Ok(PdfPageIndex::from(index))
    }

    /// Le document ne correspond plus aux octets du fichier : le prochain enregistrement le
    /// réécrira. Appelée par **toute** méthode publique qui modifie le document (appels
    /// directs du CLI compris), en plus de `apply`, dès que la modification est certaine :
    /// après les gardes de refus (`ensure_cleanable`, validation préalable), jamais avant.
    pub(crate) fn mark_changed(&mut self) {
        self.changed = true;
        self.clear_lopdf_cache();
    }

    /// Oublie la lecture `lopdf` gardée ([`Document::lopdf_view`]) : le document vient de
    /// changer (ou va changer), elle ne le décrit plus.
    pub(crate) fn clear_lopdf_cache(&mut self) {
        self.lopdf_cache.take();
    }

    /// Le document **courant** lu avec `lopdf`, gardé jusqu'à la prochaine modification : un
    /// long document ne se relit pas à chaque page. `None` si `lopdf` ne sait pas le lire, ou
    /// si PDFium n'a pas su le sérialiser ; cet échec est gardé lui aussi (pas de nouvel essai
    /// avant la prochaine modification). La lecture occupe à peu près la taille du fichier.
    ///
    /// Les octets lus sont ceux du fichier tant que rien n'a changé, sinon une sérialisation
    /// par PDFium (après un caviardage, on doit voir la page caviardée, pas l'originale).
    ///
    /// **Prend le jeton PDFium** (par [`Document::to_bytes`], quand le document a changé) :
    /// jamais sous `engine::guard()`, verrou non réentrant (interblocage).
    ///
    /// Invariant : **toute mutation du document PDFium passe d'abord par `mark_changed()`**
    /// (ou par [`Document::inner_mut`], [`Document::replace_inner`]), qui vide ce cache ; une
    /// mutation qui l'oublierait laisserait lire l'état d'avant.
    pub(crate) fn lopdf_view(&self) -> Option<&lopdf::Document> {
        // Sous l'acteur, un seul appel à la fois touche un `Document` ; ailleurs, `get_or_init`
        // resterait correct (un second appel concurrent attendrait le premier).
        self.lopdf_cache
            .get_or_init(|| {
                // [Apprentissage] `Cow` (« copie à l'écriture ») : soit un emprunt des octets
                // du fichier, soit des octets fraîchement produits, sans copier les premiers.
                // Le `?` sort de la **fermeture** (qui rend une `Option`) : un échec de
                // `to_bytes` donne `None`, gardé comme un fichier illisible.
                let bytes: Cow<'_, [u8]> = if self.changed {
                    Cow::Owned(self.to_bytes().ok()?)
                } else {
                    Cow::Borrowed(&self.file_bytes)
                };
                crate::crypt::load_limited(&bytes, self.password()).ok()
            })
            .as_ref()
    }

    /// Refuse une opération qui supprime ou remplace du contenu si le nettoyage à
    /// l'enregistrement ne pourra pas retirer ce qui est supprimé (audit E1).
    ///
    /// Pose aussi `needs_cleanup` : l'opération suit, `rewritten_bytes` vérifiera à
    /// l'enregistrement que le nettoyage a eu lieu.
    ///
    /// À appeler **avant** `engine::guard()` : `to_bytes` prend le verrou, non réentrant.
    pub(crate) fn ensure_cleanable(&mut self) -> CoreResult<()> {
        match crate::cleanup::check_cleanable(&self.to_bytes()?, self.password()) {
            Ok(()) => {
                // L'appelant va supprimer : l'enregistrement devra vérifier que le nettoyage a
                // eu lieu.
                self.needs_cleanup = true;
                Ok(())
            }
            // La cause est gardée (ADR-0005), sans répéter le préfixe « Opération impossible ».
            Err(cause) => {
                let cause = match cause {
                    CoreError::InvalidOperation(message) => message,
                    other => other.to_string(),
                };
                Err(CoreError::InvalidOperation(format!(
                    "ce fichier ne pourrait pas être nettoyé après une suppression : la \
                     suppression est refusée, l'ajout reste possible ({cause})"
                )))
            }
        }
    }

    /// Vrai si une opération qui retire une information a eu lieu depuis le dernier
    /// enregistrement (voir le champ `needs_cleanup`).
    ///
    /// Exposée uniquement pour les tests d'intégration, comme `cleanup::can_clean`.
    #[doc(hidden)]
    pub fn needs_cleanup(&self) -> bool {
        self.needs_cleanup
    }

    /// Octets du fichier tel qu'il a été chargé ou enregistré, même si le document a été
    /// modifié depuis : c'est sur eux que se vérifient les signatures numériques.
    pub fn file_bytes(&self) -> &[u8] {
        &self.file_bytes
    }

    /// Version `(majeur, mineur)` de l'en-tête des octets du fichier (`file_bytes`), pas de
    /// celle de PDFium qui ne sait écrire que 1.0 à 1.7 : elle sert de plancher à
    /// `cleanup::before_save` pour que l'édition d'un PDF 2.0 reste un PDF 2.0 (audit E2).
    pub(crate) fn file_version(&self) -> Option<(u8, u8)> {
        crate::crypt::header_version(&self.file_bytes)
    }

    /// Remplace le document PDFium sous-jacent (annuler / rétablir) ; l'ancien est libéré
    /// sous le jeton que l'appelant tient.
    pub(crate) fn replace_inner(&mut self, inner: PdfDocument<'static>) {
        self.inner = Some(inner);
        // Le jeton de police désignait l'ancien document : il ne vaut plus rien.
        self.unicode_font = None;
        self.clear_lopdf_cache();
    }

    /// Jeton de la police Unicode incorporée dans le document PDFium courant, chargée au
    /// premier besoin (l'appelant tient déjà le jeton de [`engine::guard`]).
    pub(crate) fn unicode_font_unlocked(
        &mut self,
    ) -> CoreResult<pdfium_render::prelude::PdfFontToken> {
        if let Some(token) = self.unicode_font {
            return Ok(token);
        }
        let token = self
            .inner_mut()
            .fonts_mut()
            .load_true_type_from_bytes(crate::fonts::unicode::DEJAVU_SANS, true)?;
        self.unicode_font = Some(token);
        Ok(token)
    }

    /// Mot de passe d'ouverture (pour recharger des octets issus de ce document).
    pub(crate) fn password(&self) -> Option<&str> {
        self.password.as_deref()
    }

    /// Change le mot de passe mémorisé (protection posée ou retirée, annuler / rétablir).
    pub(crate) fn set_password(&mut self, password: Option<String>) {
        self.password = password;
    }

    /// Change le chemin associé (après « Enregistrer sous »).
    pub(crate) fn set_path(&mut self, path: Option<PathBuf>) {
        self.path = path;
    }

    /// Accès mutable au document PDFium sous-jacent, réservé aux autres modules du crate.
    #[allow(clippy::expect_used)]
    pub(crate) fn inner_mut(&mut self) -> &mut PdfDocument<'static> {
        // Un accès mutable annonce une modification : la lecture `lopdf` gardée est périmée.
        self.clear_lopdf_cache();
        self.inner
            .as_mut()
            .expect("le document PDFium n'est libéré que dans Drop")
    }

    /// Accès au document PDFium sous-jacent, réservé aux autres modules du crate.
    ///
    /// PDFium laisse modifier une page obtenue par cet accès partagé : **toute mutation du
    /// document PDFium passe d'abord par `mark_changed()`**, sinon la lecture `lopdf` gardée
    /// ([`Document::lopdf_view`]) décrirait l'état d'avant.
    ///
    /// Invariant : `inner` n'est `None` que pendant `Drop`, où plus aucune méthode ne peut
    /// être appelée ; l'`expect` ne peut donc pas se déclencher (ADR-0005).
    #[allow(clippy::expect_used)]
    pub(crate) fn inner(&self) -> &PdfDocument<'static> {
        self.inner
            .as_ref()
            .expect("le document PDFium n'est libéré que dans Drop")
    }
}

/// Vrai si le fichier déclare un chiffrement par certificat (`/Filter /Adobe.PubSec`,
/// § 7.6.5) **et aucun** chiffrement standard par mot de passe. Un simple balayage des octets
/// suffit : on ne l'appelle que si PDFium a déjà refusé le fichier, pour choisir le message.
/// Un fichier à mot de passe qui mentionne `/Adobe.PubSec` (ancienne révision, commentaire)
/// reste un fichier à mot de passe (relecture de la PR B du Lot 13).
fn uses_certificate_encryption(bytes: &[u8]) -> bool {
    let contains = |needle: &[u8]| bytes.windows(needle.len()).any(|w| w == needle);
    contains(b"/Adobe.PubSec") && !contains(b"/Standard")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La lecture `lopdf` se fait une seule fois : deux lectures de la page réutilisent le
    /// même document gardé, et une modification l'oublie.
    #[test]
    fn the_lopdf_view_is_read_once_until_a_change() {
        let mut doc = Document::from_bytes(crate::testing::opentype_cff_cid_pdf(), None).unwrap();
        assert!(doc.lopdf_cache.get().is_none());
        assert_eq!(doc.page_text(0).unwrap().trim(), "除");
        let first = std::ptr::from_ref(doc.lopdf_cache.get().unwrap().as_ref().unwrap());
        assert_eq!(doc.page_text(0).unwrap().trim(), "除");
        let second = std::ptr::from_ref(doc.lopdf_view().unwrap());
        assert_eq!(first, second, "relu au lieu d'être réutilisé");
        doc.mark_changed();
        assert!(doc.lopdf_cache.get().is_none());
        // Relu depuis le document courant (sérialisé par PDFium, puisqu'il a « changé »).
        assert_eq!(doc.page_text(0).unwrap().trim(), "除");
        assert!(doc.lopdf_cache.get().is_some());
    }

    /// Un document que `lopdf` ne sait pas lire : l'échec est gardé (`Some(None)`) au lieu
    /// d'être retenté à chaque appel.
    #[test]
    fn an_unreadable_lopdf_view_is_cached_as_none() {
        let mut doc = Document::from_bytes(crate::testing::sample_pdf(1, "t"), None).unwrap();
        // Octets du fichier remplacés par du charabia : `lopdf` échoue, PDFium n'est pas relu.
        doc.file_bytes = b"%PDF-1.7 pas un PDF".to_vec();
        assert!(doc.lopdf_view().is_none());
        assert!(matches!(doc.lopdf_cache.get(), Some(None)));
    }

    /// Le drapeau `needs_cleanup` monte avec une suppression et retombe à l'enregistrement.
    #[test]
    fn needs_cleanup_is_set_by_a_deletion_and_cleared_by_a_save() {
        let mut doc =
            Document::from_bytes(crate::testing::note_with_popup_pdf("SECRET-ZX9"), None).unwrap();
        assert!(!doc.needs_cleanup);
        doc.delete_annotation(0, 0).unwrap();
        assert!(doc.needs_cleanup);
        let dir = tempfile::tempdir().unwrap();
        doc.save_as(dir.path().join("o.pdf")).unwrap();
        assert!(!doc.needs_cleanup);
    }
}
