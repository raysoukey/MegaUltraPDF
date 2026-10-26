// SPDX-License-Identifier: GPL-3.0-or-later
//! Un document PDF ouvert : métadonnées, pages, sérialisation.

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
        let bytes = std::fs::read(path)?;
        let mut doc = Self::from_bytes(bytes, password)?;
        doc.path = Some(path.to_path_buf());
        Ok(doc)
    }

    /// Charge un PDF depuis des octets déjà en mémoire.
    pub fn from_bytes(bytes: Vec<u8>, password: Option<&str>) -> CoreResult<Document> {
        let guard = engine::guard();
        let inner = match engine::load_bytes(&guard, bytes, password) {
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

    /// Dimensions de toutes les pages, dans l'ordre.
    pub fn page_sizes(&self) -> CoreResult<Vec<PageSize>> {
        (0..self.page_count()).map(|i| self.page_size(i)).collect()
    }

    /// Sérialise le document complet en octets PDF.
    pub fn to_bytes(&self) -> CoreResult<Vec<u8>> {
        let _guard = engine::guard();
        Ok(self.inner().save_to_bytes()?)
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

    /// Accès au document PDFium sous-jacent, réservé aux autres modules du crate.
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
