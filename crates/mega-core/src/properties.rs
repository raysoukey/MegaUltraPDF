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
    /// est mis à maintenant. Les métadonnées XMP éventuelles (que préfèrent les lecteurs
    /// récents) sont mises à jour pour les propriétés qu'elles contiennent déjà (C11).
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
            // Un caractère de contrôle (sauf la tabulation) n'a rien à faire dans un titre et
            // s'afficherait mal partout (C14, Lot 9).
            if let Some(v) = value
                && v.chars().any(|c| c.is_control() && c != '\t')
            {
                return Err(CoreError::InvalidOperation(format!(
                    "{name} : caractère de contrôle refusé"
                )));
            }
        }
        let bytes = self.to_bytes()?;
        // Un document chiffré est réécrit en clair puis rechiffré à l'identique (Lot 10).
        let mut session = crate::crypt::LopdfSession::load(&bytes, self.password())?;
        let doc = &mut session.doc;
        let info_id = info_id(doc);
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
        // [Apprentissage] Une fonction nommée plutôt qu'une fermeture : sa signature dit que
        // le `&str` rendu vit aussi longtemps que l'`Option` prêtée (élision des durées de
        // vie), ce que le compilateur ne déduit pas pour une fermeture.
        fn trimmed(v: &Option<String>) -> Option<&str> {
            v.as_deref().map(str::trim)
        }
        update_xmp(
            doc,
            &XmpUpdate {
                title: trimmed(&patch.title),
                author: trimmed(&patch.author),
                subject: trimmed(&patch.subject),
                keywords: trimmed(&patch.keywords),
            },
        );
        stamp_mod_date(doc);
        let out = session.finish()?;
        self.reload_from_bytes(out)
    }
}

impl Document {
    /// `/ModDate` du dictionnaire `/Info`, lu par `lopdf` (jeton déjà pris par l'appelant ;
    /// `to_bytes` n'en a pas besoin : `save_to_bytes` est appelé directement).
    fn mod_date_unlocked(&self) -> Option<String> {
        // Rien de changé : les octets du fichier suffisent, sans sérialisation complète (C15).
        let saved;
        let bytes: &[u8] = if self.changed {
            saved = self.inner().save_to_bytes().ok()?;
            &saved
        } else {
            self.file_bytes()
        };
        // Un fichier à mot de passe utilisateur se lit avec ce mot de passe (C16).
        let doc = match self.password() {
            Some(p) => {
                lopdf::Document::load_mem_with_options(bytes, lopdf::LoadOptions::with_password(p))
            }
            None => lopdf::Document::load_mem(bytes),
        }
        .ok()?;
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
    // §7.9.4 : seule l'année est obligatoire ; mois et jour absents valent 01, heures,
    // minutes et secondes 00. Les champs vont par paires après l'année.
    let count = digits
        .bytes()
        .take_while(u8::is_ascii_digit)
        .take(14)
        .count();
    if count < 4 || count % 2 != 0 {
        return None;
    }
    let mut base = digits[..count].to_string();
    base.push_str(&"0101000000"[count - 4..]);
    let naive = chrono::NaiveDateTime::parse_from_str(&base, "%Y%m%d%H%M%S").ok()?;
    let mut secs = naive.and_utc().timestamp();
    let rest = &digits[count..];
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
    } else if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        // PDF 2.0 (§7.9.2.2.1) admet aussi l'UTF-8 précédé de son BOM.
        String::from_utf8_lossy(rest).into_owned()
    } else {
        bytes.iter().map(|&b| pdf_doc_char(b)).collect()
    }
}

/// Caractère PDFDocEncoding d'un octet (ISO 32000-1, annexe D.2) : Latin-1 partout, sauf
/// 0x18-0x1F (accents isolés) et 0x80-0xA0 (puces, guillemets, ligatures, euro…).
fn pdf_doc_char(byte: u8) -> char {
    const LOW: [char; 8] = ['˘', 'ˇ', 'ˆ', '˙', '˝', '˛', '˚', '˜'];
    const HIGH: [char; 33] = [
        '•', '†', '‡', '…', '—', '–', 'ƒ', '⁄', '‹', '›', '−', '‰', '„', '“', '”', '‘', '’', '‚',
        '™', 'ﬁ', 'ﬂ', 'Ł', 'Œ', 'Š', 'Ÿ', 'Ž', 'ı', 'ł', 'œ', 'š', 'ž', '\u{FFFD}', '€',
    ];
    match byte {
        0x18..=0x1F => LOW[usize::from(byte - 0x18)],
        0x80..=0xA0 => HIGH[usize::from(byte - 0x80)],
        _ => char::from(byte),
    }
}

/// Identifiant du dictionnaire `/Info` (créé s'il manque). Un `/Info` **direct** dans la
/// remorque (permis, rare) est d'abord déplacé dans un objet indirect, ses clés gardées (C13).
pub(crate) fn info_id(doc: &mut lopdf::Document) -> lopdf::ObjectId {
    match doc.trailer.get(b"Info").ok().cloned() {
        Some(lopdf::Object::Reference(id)) if doc.get_dictionary(id).is_ok() => id,
        other => {
            let dict = match other {
                Some(lopdf::Object::Dictionary(d)) => d,
                _ => lopdf::Dictionary::new(),
            };
            let id = doc.add_object(lopdf::Object::Dictionary(dict));
            doc.trailer.set("Info", lopdf::Object::Reference(id));
            id
        }
    }
}

/// Pose `/ModDate` (et, s'il y a des métadonnées XMP, `xmp:ModifyDate` et
/// `xmp:MetadataDate`) à maintenant : appelé à chaque réécriture du fichier (C12).
pub(crate) fn stamp_mod_date(doc: &mut lopdf::Document) {
    let now = chrono::Utc::now();
    // §7.9.4 : D:YYYYMMDDHHmmSS suivi du décalage horaire (ici UTC).
    let pdf = now.format("D:%Y%m%d%H%M%S+00'00'").to_string();
    let id = info_id(doc);
    if let Ok(info) = doc.get_dictionary_mut(id) {
        info.set("ModDate", lopdf::Object::string_literal(pdf));
    }
    let xmp = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    edit_xmp(doc, |text| {
        let text = replace_simple(&text, "xmp:ModifyDate", &xmp).unwrap_or(text);
        replace_simple(&text, "xmp:MetadataDate", &xmp).unwrap_or(text)
    });
}

/// Valeurs à reporter dans les métadonnées XMP (`None` = inchangé, vide = effacé).
pub(crate) struct XmpUpdate<'a> {
    pub title: Option<&'a str>,
    pub author: Option<&'a str>,
    pub subject: Option<&'a str>,
    pub keywords: Option<&'a str>,
}

/// Reporte `update` dans le flux XMP du catalogue, pour les propriétés **déjà présentes**
/// (aucune n'est créée : pas de générateur XMP). Un flux illisible est laissé tel quel.
pub(crate) fn update_xmp(doc: &mut lopdf::Document, update: &XmpUpdate<'_>) {
    edit_xmp(doc, |mut text| {
        for (tag, value) in [
            ("dc:title", update.title),
            ("dc:creator", update.author),
            ("dc:description", update.subject),
        ] {
            if let Some(v) = value
                && let Some(new) = replace_li(&text, tag, &xml_escape(v))
            {
                text = new;
            }
        }
        if let Some(v) = update.keywords
            && let Some(new) = replace_simple(&text, "pdf:Keywords", &xml_escape(v))
        {
            text = new;
        }
        text
    });
}

/// Applique `edit` au texte du flux `/Metadata` du catalogue et le réécrit non compressé.
fn edit_xmp(doc: &mut lopdf::Document, edit: impl FnOnce(String) -> String) {
    let Some(id) = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"Metadata").ok())
        .and_then(|m| m.as_reference().ok())
    else {
        return;
    };
    let Ok(stream) = doc
        .get_object_mut(id)
        .and_then(lopdf::Object::as_stream_mut)
    else {
        return;
    };
    let data = stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone());
    let Ok(text) = String::from_utf8(data) else {
        return;
    };
    let edited = edit(text);
    stream.dict.remove(b"Filter");
    stream.dict.remove(b"DecodeParms");
    stream.set_plain_content(edited.into_bytes());
}

/// Échappe `&`, `<`, `>`, `"` et `'` pour du texte XML.
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Remplace le texte d'une entrée `<rdf:li>` de l'élément `tag` (titre, auteur, sujet : des
/// listes `rdf:Alt` / `rdf:Seq`) : celle de langue `x-default` (celle que les lecteurs
/// affichent) s'il y en a une, sinon la première ; `None` si l'élément ou l'entrée manque.
fn replace_li(xmp: &str, tag: &str, value: &str) -> Option<String> {
    let start = xmp.find(&format!("<{tag}>"))?;
    let end = start + xmp[start..].find(&format!("</{tag}>"))?;
    let default = ["xml:lang=\"x-default\"", "xml:lang='x-default'"]
        .iter()
        .find_map(|attr| xmp[start..end].find(attr))
        .and_then(|at| xmp[start..start + at].rfind("<rdf:li"));
    let li = match default {
        Some(at) => start + at,
        None => start + xmp[start..end].find("<rdf:li")?,
    };
    let open_end = li + xmp[li..end].find('>')? + 1;
    if xmp[..open_end].ends_with("/>") {
        return None;
    }
    let close = open_end + xmp[open_end..end].find("</rdf:li>")?;
    Some(format!("{}{value}{}", &xmp[..open_end], &xmp[close..]))
}

/// Remplace la valeur d'une propriété simple, écrite en élément (`<tag>v</tag>`) ou en
/// attribut (`tag="v"`) ; `None` si elle est absente.
fn replace_simple(xmp: &str, tag: &str, value: &str) -> Option<String> {
    let open = format!("<{tag}>");
    if let Some(start) = xmp.find(&open) {
        let from = start + open.len();
        let end = from + xmp[from..].find(&format!("</{tag}>"))?;
        return Some(format!("{}{value}{}", &xmp[..from], &xmp[end..]));
    }
    // Attribut entre guillemets ou entre apostrophes (les deux sont du XML valide).
    for quote in ['"', '\''] {
        let attr = format!(" {tag}={quote}");
        if let Some(found) = xmp.find(&attr) {
            let start = found + attr.len();
            let end = start + xmp[start..].find(quote)?;
            return Some(format!("{}{value}{}", &xmp[..start], &xmp[end..]));
        }
    }
    None
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

    /// Mineur du Lot 9 : dans une liste multilingue, c'est l'entrée `x-default` que les
    /// lecteurs affichent ; c'est elle qu'on remplace.
    #[test]
    fn xmp_title_replaces_the_x_default_entry() {
        let xmp = "<dc:title><rdf:Alt><rdf:li xml:lang=\"fr\">Ancien</rdf:li>\
                   <rdf:li xml:lang=\"x-default\">Old</rdf:li></rdf:Alt></dc:title>";
        let out = replace_li(xmp, "dc:title", "Nouveau").unwrap();
        assert!(out.contains("\"fr\">Ancien<"), "{out}");
        assert!(out.contains("\"x-default\">Nouveau<"), "{out}");
        // Sans `x-default`, la première entrée.
        let plain = "<dc:title><rdf:Alt><rdf:li>Old</rdf:li></rdf:Alt></dc:title>";
        assert!(
            replace_li(plain, "dc:title", "N")
                .unwrap()
                .contains("<rdf:li>N<")
        );
    }

    /// Mineur du Lot 9 : propriété en attribut entre apostrophes.
    #[test]
    fn xmp_attribute_with_single_quotes_is_replaced() {
        let xmp = "<rdf:Description pdf:Keywords='ancien'/>";
        let out = replace_simple(xmp, "pdf:Keywords", "neuf").unwrap();
        assert_eq!(out, "<rdf:Description pdf:Keywords='neuf'/>");
    }

    #[test]
    fn version_label_formats_debug_name() {
        assert_eq!(version_label("Pdf1_7"), "1.7");
        assert_eq!(version_label("Pdf2_0"), "2.0");
        assert_eq!(version_label("Unset"), "?");
    }
}

#[cfg(test)]
mod encoding_tests {
    use super::*;

    #[test]
    fn partial_pdf_dates_are_completed() {
        let at = |s: &str| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M")
                .unwrap()
                .and_utc()
                .timestamp()
        };
        // §7.9.4 : seule l'année est obligatoire ; les champs absents valent 01 ou 00.
        assert_eq!(parse_pdf_date("D:2024"), Some(at("2024-01-01 00:00")));
        assert_eq!(parse_pdf_date("D:202403"), Some(at("2024-03-01 00:00")));
        assert_eq!(parse_pdf_date("D:20240315"), Some(at("2024-03-15 00:00")));
        assert_eq!(
            parse_pdf_date("D:202403151230"),
            Some(at("2024-03-15 12:30"))
        );
        assert_eq!(
            parse_pdf_date("D:202403151230+01'00'"),
            Some(at("2024-03-15 11:30"))
        );
        assert_eq!(
            parse_pdf_date("D:20240315123045Z"),
            Some(at("2024-03-15 12:30") + 45)
        );
        assert_eq!(parse_pdf_date("D:20"), None, "année incomplète");
        assert_eq!(parse_pdf_date("D:2024031"), None, "mois tronqué");
    }

    #[test]
    fn pdfdoc_encoding_bullet_euro_and_ligatures() {
        assert_eq!(decode_text_string(&[0x80, b' ', 0xA0]), "• €");
        assert_eq!(decode_text_string(&[0x93, 0x8D, 0x8E]), "ﬁ“”");
        assert_eq!(decode_text_string(&[0x18, 0xE9]), "˘é");
    }

    #[test]
    fn utf8_bom_text_is_decoded() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("été ✓".as_bytes());
        assert_eq!(decode_text_string(&bytes), "été ✓");
    }
}
