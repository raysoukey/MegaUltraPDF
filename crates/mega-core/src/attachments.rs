// SPDX-License-Identifier: GPL-3.0-or-later
//! Pièces jointes du document (`/Names /EmbeddedFiles`, ISO 32000-2 §7.11.4) : liste,
//! extraction, ajout, suppression, par PDFium.
//!
//! \[Apprentissage\] **Un nom de fichier est une donnée non fiable** : il vient du PDF. On ne
//! s'en sert jamais pour composer un chemin d'écriture (l'utilisateur choisit le chemin de
//! sortie) ; la CLI le passe par `display_safe` avant de l'afficher.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::edit::write_atomically;
use crate::engine;
use crate::error::{CoreError, CoreResult};

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
    /// Taille des données, en octets.
    pub size: u64,
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

/// `rapport.md` → `rapport (2).md`, `rapport (3).md`… jusqu'à un nom libre.
fn unique_name(wanted: &str, existing: &[String]) -> String {
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
    /// Les pièces jointes, dans l'ordre de PDFium (au plus [`MAX_ATTACHMENTS`]).
    pub fn attachments(&self) -> CoreResult<Vec<Attachment>> {
        let _guard = engine::guard();
        Ok(self
            .inner()
            .attachments()
            .iter()
            .take(usize::from(MAX_ATTACHMENTS))
            .enumerate()
            .map(|(i, a)| Attachment {
                index: i as u16,
                name: display_name(&a.name()),
                size: a.len() as u64,
            })
            .collect())
    }

    /// Écrit les octets de la pièce jointe `index` dans `path` (écriture atomique).
    pub fn save_attachment(&self, index: u16, path: &Path) -> CoreResult<()> {
        let bytes = {
            let _guard = engine::guard();
            let attachments = self.inner().attachments();
            if index >= attachments.len() {
                return Err(invalid(format!(
                    "pièce jointe {index} introuvable (le document en a {})",
                    attachments.len()
                )));
            }
            attachments.get(index)?.save_to_bytes()?
        };
        write_atomically(path, &bytes)
    }

    /// Joint le fichier `path` sous son nom de fichier (dédoublonné si besoin).
    pub fn add_attachment(&mut self, path: &Path) -> CoreResult<()> {
        let size = std::fs::metadata(path)?.len();
        if size > MAX_ATTACHMENT_BYTES {
            return Err(invalid(format!(
                "pièce jointe trop volumineuse ({} Mo, maximum {} Mo)",
                size / (1024 * 1024),
                MAX_ATTACHMENT_BYTES / (1024 * 1024)
            )));
        }
        let wanted = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| !n.is_empty())
            .ok_or_else(|| invalid("nom de fichier invalide".to_string()))?;
        let bytes = std::fs::read(path)?;
        let existing: Vec<String> = self.attachments()?.into_iter().map(|a| a.name).collect();
        let name = unique_name(&wanted, &existing);
        let _guard = engine::guard();
        self.inner_mut()
            .attachments_mut()
            .create_attachment_from_bytes(&name, &bytes)?;
        Ok(())
    }

    /// Supprime la pièce jointe `index`.
    pub fn delete_attachment(&mut self, index: u16) -> CoreResult<()> {
        let _guard = engine::guard();
        let attachments = self.inner_mut().attachments_mut();
        if index >= attachments.len() {
            return Err(invalid(format!(
                "pièce jointe {index} introuvable (le document en a {})",
                attachments.len()
            )));
        }
        attachments.delete_at_index(index)?;
        Ok(())
    }
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
