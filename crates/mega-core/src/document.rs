// SPDX-License-Identifier: GPL-3.0-or-later
//! Un document PDF ouvert : métadonnées, pages, sérialisation.

use std::path::{Path, PathBuf};

use pdfium_render::prelude::{PdfDocument, PdfDocumentMetadataTagType, PdfPageIndex};
use serde::Serialize;

use crate::engine;
use crate::error::{CoreError, CoreResult};

/// Dimensions d'une page en points PDF (1 point = 1/72 de pouce).
///
/// Dérive `Serialize` pour traverser la frontière Rust → TypeScript telle quelle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
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
    inner: PdfDocument<'static>,
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
        let inner = engine::load_bytes(bytes, password)?;
        Ok(Document { inner, path: None })
    }

    /// Chemin d'origine, s'il a été ouvert depuis un fichier.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Titre déclaré dans les métadonnées, ou `None` s'il est absent ou vide.
    pub fn title(&self) -> Option<String> {
        self.inner
            .metadata()
            .get(PdfDocumentMetadataTagType::Title)
            .map(|tag| tag.value().trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Nombre de pages.
    pub fn page_count(&self) -> u16 {
        u16::try_from(self.inner.pages().len()).unwrap_or(u16::MAX)
    }

    /// Dimensions de la page `index` (à partir de 0).
    pub fn page_size(&self, index: u16) -> CoreResult<PageSize> {
        let rect = self.inner.pages().page_size(self.checked_index(index)?)?;
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
        Ok(self.inner.save_to_bytes()?)
    }

    /// Vérifie la borne et convertit vers le type d'index attendu par PDFium.
    pub(crate) fn checked_index(&self, index: u16) -> CoreResult<PdfPageIndex> {
        let count = self.page_count();
        if index >= count {
            return Err(CoreError::PageOutOfRange { index, count });
        }
        Ok(PdfPageIndex::from(index))
    }

    /// Accès au document PDFium sous-jacent, réservé aux autres modules du crate.
    pub(crate) fn inner(&self) -> &PdfDocument<'static> {
        &self.inner
    }
}
