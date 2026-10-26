// SPDX-License-Identifier: GPL-3.0-or-later
//! Le type d'erreur unique de `mega-core`.
//!
//! \[Apprentissage\] En Rust, les erreurs sont des valeurs ordinaires retournées dans un
//! `Result`. Une bibliothèque définit en général **une** énumération d'erreurs (ici avec
//! la crate `thiserror`, qui génère `Display` et `Error` à partir des attributs), tandis
//! qu'une application utilise `anyhow` pour les agréger sans les typer finement.

use std::path::PathBuf;

use pdfium_render::prelude::{PdfiumError, PdfiumInternalError};

/// Toutes les erreurs que `mega-core` peut signaler.
///
/// Chaque variante porte un message en français destiné à être affiché tel quel
/// à l'utilisateur (ligne de commande ou interface).
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// La bibliothèque dynamique PDFium n'a été trouvée dans aucun des dossiers essayés.
    #[error(
        "Bibliothèque PDFium introuvable. Lancez `cargo xtask fetch-pdfium`. \
         Chemins essayés : {}",
        format_paths(searched)
    )]
    LibraryNotFound {
        /// Dossiers inspectés, dans l'ordre.
        searched: Vec<PathBuf>,
    },

    /// Le document est chiffré et aucun mot de passe valide n'a été fourni.
    #[error("Ce document est protégé par un mot de passe.")]
    PasswordRequired,

    /// Les octets fournis ne forment pas un PDF lisible.
    #[error("Ce fichier n'est pas un PDF valide ({0}).")]
    InvalidPdf(String),

    /// Un index de page dépasse le nombre de pages du document.
    #[error("Page {index} hors limites (le document a {count} pages).")]
    PageOutOfRange {
        /// Index demandé (à partir de 0).
        index: u16,
        /// Nombre de pages du document.
        count: u16,
    },

    /// Erreur d'entrée/sortie (fichier introuvable, droits insuffisants…).
    ///
    /// `#[from]` génère `impl From<std::io::Error> for CoreError`, ce qui permet
    /// d'écrire `std::fs::read(path)?` directement dans une fonction qui retourne
    /// `CoreResult`.
    #[error("Erreur d'accès au fichier : {0}")]
    Io(#[from] std::io::Error),

    /// Toute autre erreur remontée par PDFium, conservée sous forme de texte.
    #[error("Erreur interne de PDFium : {0}")]
    Pdfium(String),
}

/// Raccourci pour `Result<T, CoreError>`, comme `io::Result` dans la bibliothèque standard.
pub type CoreResult<T> = Result<T, CoreError>;

fn format_paths(paths: &[PathBuf]) -> String {
    if paths.is_empty() {
        return "(aucun)".to_string();
    }
    paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

impl From<PdfiumError> for CoreError {
    /// Traduit les erreurs de PDFium en variantes parlantes pour nos utilisateurs.
    fn from(e: PdfiumError) -> Self {
        match e {
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
                CoreError::PasswordRequired
            }
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FormatError) => {
                CoreError::InvalidPdf("format non reconnu".to_string())
            }
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FileError) => {
                CoreError::Io(std::io::Error::other("PDFium n'a pas pu lire le fichier"))
            }
            PdfiumError::IoError(io) => CoreError::Io(io),
            other => CoreError::Pdfium(format!("{other:?}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_library_error_mentions_xtask() {
        let e = CoreError::LibraryNotFound { searched: vec![] };
        assert!(e.to_string().contains("cargo xtask fetch-pdfium"));
    }
}
