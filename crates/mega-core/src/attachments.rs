// SPDX-License-Identifier: GPL-3.0-or-later
//! Pièces jointes du document (`/Names /EmbeddedFiles`, ISO 32000-2 §7.11.4) : liste,
//! extraction, ajout, suppression, par PDFium.
//!
//! \[Apprentissage\] **Un nom de fichier est une donnée non fiable** : il vient du PDF. On ne
//! s'en sert jamais pour composer un chemin d'écriture (l'utilisateur choisit le chemin de
//! sortie) ; la CLI le passe par `display_safe` avant de l'afficher.

use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::edit::write_atomically;
use crate::engine;
use crate::error::{CoreError, CoreResult};
use crate::properties::decode_text_string;

/// Taille maximale d'une pièce jointe ajoutée (octets).
pub const MAX_ATTACHMENT_BYTES: u64 = 100 * 1024 * 1024;
/// Nombre maximal de pièces jointes listées.
pub const MAX_ATTACHMENTS: u16 = 1000;

/// Une pièce jointe telle que l'interface la présente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    /// Position dans la liste (identifiant pour extraire ou supprimer).
    pub index: u16,
    /// Nom de fichier déclaré dans le PDF, nettoyé (dernier composant, sans caractère de
    /// contrôle ni d'inversion bidi, au plus 255 caractères) : voir `display_name`.
    pub name: String,
    /// Taille des données, en octets. Pour une pièce jointe de page sans `/Params /Size`,
    /// la taille du flux tel qu'il est stocké (comprimé, le cas échéant) : la liste ne
    /// décompresse rien.
    pub size: u64,
    /// Page (à partir de 0) dont l'annotation `FileAttachment` porte ce fichier ; `None`
    /// pour un fichier joint au document (`/Names /EmbeddedFiles`).
    pub page: Option<u16>,
}

/// Une pièce jointe de page, lue avec `lopdf` : où elle est et ce qu'elle contient.
struct PageAttachment {
    page: u16,
    /// Index de l'annotation dans le `/Annots` brut de la page.
    annot_index: u16,
    name: String,
    size: u64,
    /// Numéro du flux `/EF /F`, pour l'extraire sans reparcourir le document.
    stream: lopdf::ObjectId,
}

fn invalid(msg: String) -> CoreError {
    CoreError::InvalidOperation(msg)
}

/// Nom de pièce jointe sûr à afficher et à proposer à l'enregistrement : seulement le dernier
/// composant (après `/`, `\` ou `:`), sans caractère de contrôle ni d'inversion bidi
/// (U+202A-U+202E, U+2066-U+2069, U+200E-U+200F), au plus 255 caractères.
///
/// Le nom déclaré dans un PDF est une donnée non fiable : « C:\…\Startup\maj.bat » ouvrirait
/// le dialogue d'enregistrement dans le dossier de démarrage, et U+202E maquillerait un
/// « .exe » en « .pdf » dans le panneau.
pub(crate) fn display_name(raw: &str) -> String {
    let last = raw.rsplit(['/', '\\', ':']).next().unwrap_or("");
    let cleaned: String = last
        .chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(*c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}')
        })
        .take(255)
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() || trimmed.chars().all(|c| c == '.') {
        "piece-jointe".to_string()
    } else {
        trimmed.to_string()
    }
}

/// `rapport.md` → `rapport (2).md`, `rapport (3).md`… jusqu'à un nom libre. Sert aussi à la
/// fusion, qui ne doit pas confondre deux pièces jointes de même nom (`pages::carry`).
pub(crate) fn unique_name(wanted: &str, existing: &[String]) -> String {
    if !existing.iter().any(|n| n == wanted) {
        return wanted.to_string();
    }
    let (stem, ext) = match wanted.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, format!(".{e}")),
        _ => (wanted, String::new()),
    };
    (2..)
        .map(|n| format!("{stem} ({n}){ext}"))
        .find(|candidate| !existing.iter().any(|n| n == candidate))
        .unwrap_or_else(|| wanted.to_string())
}

impl Document {
    /// Les pièces jointes : d'abord celles du document (`/EmbeddedFiles`, dans l'ordre de
    /// PDFium), puis celles que portent les annotations `FileAttachment` de chaque page
    /// (audit L11), au plus [`MAX_ATTACHMENTS`] en tout.
    pub fn attachments(&self) -> CoreResult<Vec<Attachment>> {
        let mut list: Vec<Attachment> = {
            let _guard = engine::guard();
            self.inner()
                .attachments()
                .iter()
                .take(usize::from(MAX_ATTACHMENTS))
                .enumerate()
                .map(|(i, a)| Attachment {
                    index: i as u16,
                    name: display_name(&a.name()),
                    size: a.len() as u64,
                    page: None,
                })
                .collect()
        };
        let room = usize::from(MAX_ATTACHMENTS) - list.len();
        for entry in self.page_attachments(room) {
            list.push(Attachment {
                index: list.len() as u16,
                name: entry.name,
                size: entry.size,
                page: Some(entry.page),
            });
        }
        Ok(list)
    }

    /// Nombre de fichiers joints au document (hors annotations), plafonné.
    fn embedded_count(&self) -> usize {
        let _guard = engine::guard();
        self.inner().attachments().len().min(MAX_ATTACHMENTS).into()
    }

    /// Les `FileAttachment` des pages, au plus `limit`, dans l'ordre des pages puis de
    /// `/Annots`. Lecture `lopdf` (PDFium ne les expose pas) ; un document que `lopdf` ne lit
    /// pas, ou dont l'arbre de pages ne compte pas comme PDFium, n'en donne aucune.
    fn page_attachments(&self, limit: usize) -> Vec<PageAttachment> {
        let mut found = Vec::new();
        if limit == 0 {
            return found;
        }
        let Some(doc) = self.lopdf_view() else {
            return found;
        };
        let pages = doc.get_pages();
        if pages.len() != usize::from(self.page_count()) {
            return found;
        }
        for (number, page_id) in pages {
            let Some(annots) = doc
                .get_dictionary(page_id)
                .ok()
                .and_then(|p| p.get(b"Annots").ok())
                .and_then(|o| doc.dereference(o).ok())
                .and_then(|(_, o)| o.as_array().ok())
            else {
                continue;
            };
            for (i, item) in annots
                .iter()
                .take(crate::annotations::MAX_ANNOTATIONS_PER_PAGE)
                .enumerate()
            {
                let Some(dict) = doc
                    .dereference(item)
                    .ok()
                    .and_then(|(_, o)| o.as_dict().ok())
                else {
                    continue;
                };
                if dict.get(b"Subtype").and_then(|s| s.as_name()).ok() != Some(b"FileAttachment") {
                    continue;
                }
                let Some((name, size, stream)) = file_attachment(doc, dict) else {
                    continue;
                };
                // Numéros de page à partir de 1 ; `from_bytes` refuse plus de 65 535 pages, la
                // conversion ne peut donc échouer que sur un arbre de pages aberrant.
                let (Some(page), Ok(annot_index)) = (
                    number.checked_sub(1).and_then(|n| u16::try_from(n).ok()),
                    u16::try_from(i),
                ) else {
                    continue;
                };
                found.push(PageAttachment {
                    page,
                    annot_index,
                    name,
                    size,
                    stream,
                });
                if found.len() >= limit {
                    return found;
                }
            }
        }
        found
    }

    /// Écrit les octets de la pièce jointe `index` dans `path` (écriture atomique).
    pub fn save_attachment(&self, index: u16, path: &Path) -> CoreResult<()> {
        let embedded = self.embedded_count();
        let bytes = if usize::from(index) < embedded {
            let _guard = engine::guard();
            self.inner().attachments().get(index)?.save_to_bytes()?
        } else {
            let wanted = usize::from(index) - embedded;
            let entry = self
                .page_attachments(wanted + 1)
                .into_iter()
                .nth(wanted)
                .ok_or_else(|| not_found(index, embedded))?;
            self.lopdf_view()
                .and_then(|doc| doc.get_object(entry.stream).ok())
                .and_then(|o| o.as_stream().ok())
                .and_then(crate::crypt::stream_bytes)
                .ok_or_else(|| invalid("pièce jointe illisible ou trop volumineuse".to_string()))?
        };
        write_atomically(path, &bytes)
    }

    /// Joint le fichier `path` sous son nom de fichier (dédoublonné si besoin).
    pub fn add_attachment(&mut self, path: &Path) -> CoreResult<()> {
        self.mark_changed();
        // Une seule lecture, bornée : le fichier ne peut pas grossir entre une vérification de
        // taille et sa lecture (C7, Lot 9).
        // [Apprentissage] `Read::take(n)` limite ce qu'un lecteur peut rendre à n octets.
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(MAX_ATTACHMENT_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_ATTACHMENT_BYTES {
            return Err(invalid(format!(
                "pièce jointe trop volumineuse (plus de {} Mo)",
                MAX_ATTACHMENT_BYTES / (1024 * 1024)
            )));
        }
        let wanted = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| !n.is_empty())
            .ok_or_else(|| invalid("nom de fichier invalide".to_string()))?;
        let existing: Vec<String> = self.attachments()?.into_iter().map(|a| a.name).collect();
        let name = unique_name(&wanted, &existing);
        let _guard = engine::guard();
        self.inner_mut()
            .attachments_mut()
            .create_attachment_from_bytes(&name, &bytes)?;
        Ok(())
    }

    /// Supprime la pièce jointe `index` ; pour une pièce jointe de page, c'est son
    /// annotation qui disparaît.
    pub fn delete_attachment(&mut self, index: u16) -> CoreResult<()> {
        // Le flux du fichier joint deviendrait orphelin : seul le nettoyage le retire.
        self.ensure_cleanable()?;
        let embedded = self.embedded_count();
        if usize::from(index) >= embedded {
            let wanted = usize::from(index) - embedded;
            let entry = self
                .page_attachments(wanted + 1)
                .into_iter()
                .nth(wanted)
                .ok_or_else(|| not_found(index, embedded))?;
            // `lopdf` et PDFium doivent désigner la même annotation : on vérifie le type avant
            // de supprimer (une entrée nulle ou une sérialisation divergente décalerait l'index).
            let is_attachment = {
                let _guard = engine::guard();
                let page_index = self.checked_index_unlocked(entry.page)?;
                let page = self.inner().pages().get(page_index)?;
                let annotations = page.annotations();
                usize::from(entry.annot_index) < annotations.len()
                    && annotations
                        .get(usize::from(entry.annot_index))
                        .is_ok_and(|a| {
                            a.annotation_type()
                                == pdfium_render::prelude::PdfPageAnnotationType::FileAttachment
                        })
            };
            if !is_attachment {
                return Err(invalid(
                    "pièce jointe de page introuvable à cet index".to_string(),
                ));
            }
            // La garde a déjà eu lieu en tête : pas une seconde lecture de tout le document.
            return self.delete_annotation_after_guard(entry.page, entry.annot_index);
        }
        self.mark_changed();
        let _guard = engine::guard();
        self.inner_mut().attachments_mut().delete_at_index(index)?;
        Ok(())
    }
}

fn not_found(index: u16, embedded: usize) -> CoreError {
    invalid(format!(
        "pièce jointe {index} introuvable (le document en a au moins {embedded})"
    ))
}

/// Nom nettoyé, taille et flux d'une annotation `FileAttachment` (`/FS` : nom `/UF` puis
/// `/F`, données `/EF /F`). `None` si la spécification n'a pas de flux lisible.
fn file_attachment(
    doc: &lopdf::Document,
    annot: &lopdf::Dictionary,
) -> Option<(String, u64, lopdf::ObjectId)> {
    let (_, fs) = doc.dereference(annot.get(b"FS").ok()?).ok()?;
    let fs = fs.as_dict().ok()?;
    let raw_name = ["UF", "F"].iter().find_map(|key| {
        let (_, value) = doc.dereference(fs.get(key.as_bytes()).ok()?).ok()?;
        value.as_str().ok().map(decode_text_string)
    });
    let (_, ef) = doc.dereference(fs.get(b"EF").ok()?).ok()?;
    let ef = ef.as_dict().ok()?;
    let stream_id = ["F", "UF"]
        .iter()
        .find_map(|key| ef.get(key.as_bytes()).ok()?.as_reference().ok())?;
    let stream = doc.get_object(stream_id).ok()?.as_stream().ok()?;
    let declared = stream
        .dict
        .get(b"Params")
        .ok()
        .and_then(|p| doc.dereference(p).ok())
        .and_then(|(_, p)| p.as_dict().ok())
        .and_then(|p| p.get(b"Size").ok())
        .and_then(|s| s.as_i64().ok())
        .and_then(|s| u64::try_from(s).ok())
        // Une taille déclarée absurde (donnée non fiable) ne s'affiche pas telle quelle.
        .filter(|s| *s <= 256 * 1024 * 1024);
    // Sans taille déclarée, la longueur **brute** du flux (comprimée, le cas échéant) : lister
    // les pièces jointes ne décompresse pas chacune d'elles (une « bombe » ne se déplie qu'à
    // l'enregistrement, plafonné par `stream_bytes`).
    let size = declared.unwrap_or(stream.content.len() as u64);
    Some((display_name(&raw_name.unwrap_or_default()), size, stream_id))
}

#[cfg(test)]
mod tests {
    use super::unique_name;

    /// I6 (relecture) : le nom vient du PDF ; il sert d'étiquette et de nom proposé à
    /// l'enregistrement, donc jamais de chemin ni de caractère de contrôle ou d'inversion.
    #[test]
    fn display_name_keeps_only_a_safe_file_name() {
        use super::display_name;
        assert_eq!(display_name("notes.txt"), "notes.txt");
        assert_eq!(
            display_name("C:\\Users\\x\\Start Menu\\Programs\\Startup\\maj.bat"),
            "maj.bat"
        );
        assert_eq!(display_name("../../etc/passwd"), "passwd");
        assert_eq!(display_name("C:evil.exe"), "evil.exe");
        assert_eq!(display_name("facture\u{202E}fdp.exe"), "facturefdp.exe");
        assert_eq!(display_name("a\u{0}b\u{7}c"), "abc");
        assert_eq!(display_name("///"), "piece-jointe");
        assert_eq!(display_name(""), "piece-jointe");
        assert_eq!(display_name(".."), "piece-jointe");
    }

    #[test]
    fn unique_name_numbers_duplicates() {
        let existing = vec![
            "a.txt".to_string(),
            "a (2).txt".to_string(),
            "sans".to_string(),
        ];
        assert_eq!(unique_name("b.txt", &existing), "b.txt");
        assert_eq!(unique_name("a.txt", &existing), "a (3).txt");
        assert_eq!(unique_name("sans", &existing), "sans (2)");
        assert_eq!(
            unique_name(".bashrc", &[".bashrc".to_string()]),
            ".bashrc (2)"
        );
    }
}
