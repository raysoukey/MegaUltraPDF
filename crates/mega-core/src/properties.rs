// SPDX-License-Identifier: GPL-3.0-or-later
//! Propriétés d'un document : métadonnées et version du format.

use pdfium_render::prelude::PdfDocumentMetadataTagType as Tag;
use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::engine;
use crate::error::CoreResult;

/// Les métadonnées lisibles d'un document (dictionnaire `/Info`) et sa version PDF.
///
/// Les dates sont renvoyées **brutes** au format PDF (`D:YYYYMMDDHHmmSS…`) ; le frontend
/// les met en forme. `None` quand la clé est absente ou vide.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DocumentProperties {
    /// `/Title`.
    pub title: Option<String>,
    /// `/Author`.
    pub author: Option<String>,
    /// `/Subject`.
    pub subject: Option<String>,
    /// `/Keywords`.
    pub keywords: Option<String>,
    /// `/Creator` (application d'origine).
    pub creator: Option<String>,
    /// `/Producer` (bibliothèque qui a écrit le PDF).
    pub producer: Option<String>,
    /// `/CreationDate`, brute.
    pub creation_date: Option<String>,
    /// `/ModDate`, brute.
    pub modification_date: Option<String>,
    /// Version du format, ex. `"1.7"` (`"?"` si inconnue).
    pub pdf_version: String,
    /// Nombre de pages.
    pub page_count: u16,
}

impl Document {
    /// Lit les propriétés du document.
    pub fn properties(&self) -> CoreResult<DocumentProperties> {
        let _guard = engine::guard();
        let meta = self.inner().metadata();
        let get = |tag: Tag| -> Option<String> {
            meta.get(tag)
                .map(|t| t.value().trim().to_string())
                .filter(|s| !s.is_empty())
        };
        Ok(DocumentProperties {
            title: get(Tag::Title),
            author: get(Tag::Author),
            subject: get(Tag::Subject),
            keywords: get(Tag::Keywords),
            creator: get(Tag::Creator),
            producer: get(Tag::Producer),
            creation_date: get(Tag::CreationDate),
            modification_date: get(Tag::ModificationDate),
            pdf_version: version_label(&format!("{:?}", self.inner().version())),
            page_count: self.count_unlocked(),
        })
    }
}

/// `"Pdf1_7"` → `"1.7"`, `"Unset"` → `"?"`.
fn version_label(debug: &str) -> String {
    match debug.strip_prefix("Pdf") {
        Some(rest) => rest.replace('_', "."),
        None => "?".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_label_formats_debug_name() {
        assert_eq!(version_label("Pdf1_7"), "1.7");
        assert_eq!(version_label("Pdf2_0"), "2.0");
        assert_eq!(version_label("Unset"), "?");
    }
}
