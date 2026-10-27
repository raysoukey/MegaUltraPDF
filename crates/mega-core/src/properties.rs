// SPDX-License-Identifier: GPL-3.0-or-later
//! Propriétés d'un document : métadonnées et version du format.

use pdfium_render::prelude::PdfDocumentMetadataTagType as Tag;
use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::engine;
use crate::error::{CoreError, CoreResult};

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
            // pdfium-render 0.9.4 demande à PDFium la balise « ModificationDate », qui
            // n'existe pas (la clé PDF est `/ModDate`) : toujours vide. On la lit nous-mêmes.
            modification_date: get(Tag::ModificationDate).or_else(|| self.mod_date_unlocked()),
            pdf_version: version_label(&format!("{:?}", self.inner().version())),
            page_count: self.count_unlocked(),
        })
    }
}

/// Longueur maximale (caractères) d'un champ de métadonnées écrit.
pub const MAX_METADATA_CHARS: usize = 1024;

/// Modification partielle des métadonnées : `None` = inchangé, chaîne vide = clé retirée.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MetadataPatch {
    /// Titre (`/Title`).
    pub title: Option<String>,
    /// Auteur (`/Author`).
    pub author: Option<String>,
    /// Sujet (`/Subject`).
    pub subject: Option<String>,
    /// Mots-clés (`/Keywords`).
    pub keywords: Option<String>,
}

impl Document {
    /// Écrit les métadonnées (`/Info`) par `lopdf` puis recharge le document ; `/ModDate`
    /// est mis à maintenant. Le dictionnaire XMP éventuel n'est pas touché (documenté).
    ///
    /// \[Apprentissage\] pdfium-render ne sait pas écrire les métadonnées (voir
    /// `testing.rs`) : c'est le cas d'école de l'ADR-0003, un moteur pour lire et afficher,
    /// l'autre pour poser exactement les clés que la norme décrit (§14.3.3).
    pub fn set_metadata(&mut self, patch: &MetadataPatch) -> CoreResult<()> {
        self.mark_changed();
        for (name, value) in [
            ("titre", &patch.title),
            ("auteur", &patch.author),
            ("sujet", &patch.subject),
            ("mots-clés", &patch.keywords),
        ] {
            if let Some(v) = value
                && v.chars().count() > MAX_METADATA_CHARS
            {
                return Err(CoreError::InvalidOperation(format!(
                    "{name} : {MAX_METADATA_CHARS} caractères au plus"
                )));
            }
        }
        let bytes = self.to_bytes()?;
        let mut doc = lopdf::Document::load_mem(&bytes).map_err(|e| {
            CoreError::InvalidOperation(format!("structure du PDF illisible : {e}"))
        })?;
        if doc.is_encrypted() || doc.was_encrypted() {
            return Err(CoreError::InvalidOperation(
                "métadonnées d'un PDF chiffré : non modifiables dans cette version".to_string(),
            ));
        }
        let info_id = match doc
            .trailer
            .get(b"Info")
            .ok()
            .and_then(|o| o.as_reference().ok())
        {
            Some(id) if doc.get_dictionary(id).is_ok() => id,
            _ => {
                let id = doc.add_object(lopdf::Object::Dictionary(lopdf::Dictionary::new()));
                doc.trailer.set("Info", lopdf::Object::Reference(id));
                id
            }
        };
        let info = doc
            .get_dictionary_mut(info_id)
            .map_err(|e| CoreError::InvalidOperation(format!("dictionnaire /Info : {e}")))?;
        for (key, value) in [
            ("Title", &patch.title),
            ("Author", &patch.author),
            ("Subject", &patch.subject),
            ("Keywords", &patch.keywords),
        ] {
            match value.as_deref().map(str::trim) {
                None => {}
                Some("") => {
                    info.remove(key.as_bytes());
                }
                Some(text) => info.set(key, crate::forms::write::pdf_text(text)),
            }
        }
        // §7.9.4 : D:YYYYMMDDHHmmSS suivi du décalage horaire (ici UTC).
        let now = chrono::Utc::now()
            .format("D:%Y%m%d%H%M%S+00'00'")
            .to_string();
        info.set("ModDate", lopdf::Object::string_literal(now));
        let mut out = Vec::new();
        doc.save_to(&mut out)?;
        self.reload_from_bytes(out)
    }
}

impl Document {
    /// `/ModDate` du dictionnaire `/Info`, lu par `lopdf` (jeton déjà pris par l'appelant ;
    /// `to_bytes` n'en a pas besoin : `save_to_bytes` est appelé directement).
    fn mod_date_unlocked(&self) -> Option<String> {
        let bytes = self.inner().save_to_bytes().ok()?;
        let doc = lopdf::Document::load_mem(&bytes).ok()?;
        let info = doc.trailer.get(b"Info").ok()?;
        let (_, info) = doc.dereference(info).ok()?;
        let raw = info.as_dict().ok()?.get(b"ModDate").ok()?;
        let (_, raw) = doc.dereference(raw).ok()?;
        let text = decode_text_string(raw.as_str().ok()?);
        let text = text.trim().to_string();
        (!text.is_empty()).then_some(text)
    }
}

/// Instant Unix d'une date PDF `D:AAAAMMJJHHmmSS` (le décalage horaire est appliqué s'il est
/// écrit, ignoré s'il est mal formé).
pub(crate) fn parse_pdf_date(text: &str) -> Option<i64> {
    let digits = text.strip_prefix("D:").unwrap_or(text);
    let base = digits.get(..14)?;
    let naive = chrono::NaiveDateTime::parse_from_str(base, "%Y%m%d%H%M%S").ok()?;
    let mut secs = naive.and_utc().timestamp();
    let rest = &digits[14..];
    let sign = match rest.chars().next() {
        Some('+') => -1,
        Some('-') => 1,
        _ => 0,
    };
    if sign != 0 {
        let hh: i64 = rest.get(1..3).and_then(|s| s.parse().ok()).unwrap_or(0);
        let mm: i64 = rest.get(4..6).and_then(|s| s.parse().ok()).unwrap_or(0);
        secs += sign * (hh * 3600 + mm * 60);
    }
    Some(secs)
}

/// Chaîne de texte PDF (§7.9.2.2) : UTF-16BE si elle commence par le BOM `FE FF`, sinon
/// PDFDocEncoding, dont les 256 valeurs coïncident avec Latin-1 pour tout ce qu'une date ou
/// un titre ordinaire contient.
pub(crate) fn decode_text_string(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let (pairs, _odd) = rest.as_chunks::<2>();
        let units: Vec<u16> = pairs.iter().map(|c| u16::from_be_bytes(*c)).collect();
        String::from_utf16_lossy(&units)
    } else {
        bytes.iter().map(|&b| char::from(b)).collect()
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
