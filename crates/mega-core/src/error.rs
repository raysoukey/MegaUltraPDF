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

    /// Le document est chiffré et aucun mot de passe n'a été fourni.
    #[error("Ce document est protégé par un mot de passe.")]
    PasswordRequired,

    /// Le document est chiffré et le mot de passe fourni ne l'ouvre pas.
    ///
    /// PDFium renvoie le même code dans les deux cas ; c'est `Document::from_bytes` qui
    /// distingue « aucun mot de passe » de « mauvais mot de passe ».
    #[error("Mot de passe incorrect.")]
    WrongPassword,

    /// Les octets fournis ne forment pas un PDF lisible.
    #[error("Ce fichier n'est pas un PDF valide ({0}).")]
    InvalidPdf(String),

    /// Le document est chiffré d'une façon qu'aucun mot de passe n'ouvre (par certificat,
    /// `/Adobe.PubSec`) : le dire plutôt que demander un mot de passe en boucle (audit du
    /// Lot 13, C9).
    #[error("Ce document est chiffré {0} : MegaUltraPDF ne sait pas l'ouvrir.")]
    UnsupportedEncryption(String),

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
    /// `CoreResult`. Il marque aussi l'`io::Error` comme **cause** (`source()`) :
    /// c'est pourquoi le message ne la répète pas (sinon `anyhow` l'afficherait deux fois).
    /// Pour un message complet, voir [`display_chain`].
    #[error("Erreur d'accès au fichier")]
    Io(#[from] std::io::Error),

    /// Le fichier dépasse la taille qu'un document peut avoir ([`MAX_FILE_BYTES`]) : refusé
    /// avant d'être lu (audit du Lot 13, R6).
    #[error(
        "Fichier trop volumineux : {} Mo (maximum {} Mo).",
        bytes / 1_000_000,
        MAX_FILE_BYTES / 1_000_000
    )]
    FileTooLarge {
        /// Taille du fichier, en octets.
        bytes: u64,
    },

    /// Le fichier à remplacer est en lecture seule (audit du Lot 13, P4).
    #[error(
        "Ce fichier est en lecture seule : utilisez « Enregistrer sous » pour l'écrire ailleurs."
    )]
    FileReadOnly,

    /// Le fichier à remplacer est ouvert par une autre application qui en interdit l'écriture
    /// (Acrobat, un traitement de texte…) (audit du Lot 13, P4).
    #[error(
        "Ce fichier est utilisé par une autre application : fermez-la, ou utilisez « Enregistrer sous »."
    )]
    FileLocked,

    /// Le dossier de destination n'accepte pas l'écriture (droits, support en lecture seule)
    /// (audit du Lot 13, P4).
    #[error(
        "Impossible d'écrire dans ce dossier : utilisez « Enregistrer sous » pour choisir un autre emplacement."
    )]
    FolderNotWritable,

    /// Le rendu demandé dépasserait la taille d'image maximale autorisée.
    #[error(
        "Rendu trop grand : {width_px} × {height_px} px (maximum {} Mpx). Réduisez l'échelle.",
        MAX_RENDER_PIXELS / 1_000_000
    )]
    RenderTooLarge {
        /// Largeur demandée en pixels.
        width_px: u64,
        /// Hauteur demandée en pixels.
        height_px: u64,
    },

    /// Toute autre erreur remontée par PDFium, conservée sous forme de texte.
    #[error("Erreur interne de PDFium : {0}")]
    Pdfium(String),

    /// L'identifiant ne correspond à aucun document ouvert (fermé ou jamais ouvert).
    #[error("Document inconnu ({id}).")]
    NotFound {
        /// Identifiant demandé.
        id: u32,
    },

    /// Le rendu demandé appartient à une génération périmée (zoom ou rotation changés
    /// depuis) : il est abandonné sans travail. Le frontend ignore simplement la réponse.
    #[error("Rendu annulé (génération périmée).")]
    Cancelled,

    /// Le thread de l'acteur PDFium s'est arrêté : plus aucune opération possible.
    #[error("Le service PDF est arrêté.")]
    ServiceStopped,

    /// L'opération a échoué de façon inattendue (panique interne). Si elle modifiait un
    /// document, celui-ci a été fermé par précaution ; les autres restent ouverts (audit du
    /// Lot 13, R4).
    #[error(
        "L'opération a échoué de façon inattendue. Si elle modifiait le document, il a été \
         fermé par précaution : rouvrez-le pour continuer."
    )]
    Aborted,

    /// Une spécification de pages (« 1-3,5 ») est mal formée ou hors des pages du document.
    #[error("Plage de pages invalide : {0}")]
    InvalidPageSpec(String),

    /// Une opération confiée à un programme externe (Tesseract) n'a pas abouti dans le délai :
    /// le programme a été arrêté (audit du Lot 13, R2).
    #[error("{operation} : délai de {seconds} s dépassé, opération abandonnée.")]
    Timeout {
        /// L'opération, en français (« Reconnaissance de texte (page 3) »).
        operation: String,
        /// Le délai accordé, en secondes.
        seconds: u64,
    },

    /// L'opération demandée n'a pas de sens dans l'état actuel du document
    /// (supprimer toutes les pages, enregistrer un document sans chemin…).
    #[error("Opération impossible : {0}")]
    InvalidOperation(String),
}

/// Raccourci pour `Result<T, CoreError>`, comme `io::Result` dans la bibliothèque standard.
pub type CoreResult<T> = Result<T, CoreError>;

/// Taille maximale d'un fichier PDF ouvert : 2 Gio moins un octet. PDFium reçoit la longueur
/// d'un document en mémoire dans un entier signé 32 bits (`int`) : au-delà, il ne peut pas le
/// charger, et lire le fichier en entier ne servirait qu'à épuiser la mémoire.
pub const MAX_FILE_BYTES: u64 = i32::MAX as u64;

/// Nombre maximal de pixels qu'un rendu peut produire (50 mégapixels, soit 200 Mo en RGBA).
///
/// Une page PDF peut mesurer jusqu'à 14 400 pt de côté ; sans borne, un zoom fort
/// demanderait des dizaines de gigaoctets à PDFium.
pub const MAX_RENDER_PIXELS: u64 = 50_000_000;

/// Message complet d'une erreur : son texte, puis celui de chacune de ses causes.
///
/// \[Apprentissage\] Le trait `std::error::Error` a une méthode `source()` qui renvoie la
/// cause sous-jacente, si elle existe. `Display` ne montre que le dernier maillon ; cette
/// fonction remonte la chaîne, comme le fait `anyhow` avec `{e:#}`.
pub fn display_chain(err: &dyn std::error::Error) -> String {
    let mut text = err.to_string();
    let mut cause = err.source();
    while let Some(c) = cause {
        text.push_str(" : ");
        text.push_str(&c.to_string());
        cause = c.source();
    }
    text
}

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

    /// Relecture de la PR C du Lot 13 : un message coupé sur deux lignes du source gardait
    /// une suite d'espaces au milieu de la phrase. Aucun message ne doit en contenir.
    #[test]
    fn messages_have_no_runs_of_spaces() {
        let errors = [
            CoreError::FileReadOnly,
            CoreError::FileLocked,
            CoreError::FolderNotWritable,
            CoreError::Aborted,
            CoreError::FileTooLarge {
                bytes: 3_000_000_000,
            },
            CoreError::UnsupportedEncryption("par certificat".to_string()),
            CoreError::Timeout {
                operation: "OCR".to_string(),
                seconds: 300,
            },
        ];
        for e in errors {
            let text = e.to_string();
            assert!(!text.contains("  "), "{text:?}");
        }
    }

    #[test]
    fn missing_library_error_mentions_xtask() {
        let e = CoreError::LibraryNotFound { searched: vec![] };
        assert!(e.to_string().contains("cargo xtask fetch-pdfium"));
    }
}
