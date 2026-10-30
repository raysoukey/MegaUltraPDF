// SPDX-License-Identifier: GPL-3.0-or-later
//! Erreurs renvoyées au frontend par les commandes Tauri.
//!
//! Tauri sérialise la valeur `Err` d'une commande et la livre au JavaScript comme
//! valeur de rejet de la promesse. On lui donne une forme stable `{ kind, message }`
//! que le frontend peut tester (`kind === 'password_required'`) et afficher.

use mega_core::CoreError;
use serde::Serialize;

/// Erreur applicative sérialisable, image côté Tauri de [`CoreError`].
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    /// Catégorie stable, reflétée dans `api.ts`.
    pub kind: ErrorKind,
    /// Message en français, prêt à afficher.
    pub message: String,
}

/// Catégories d'erreur connues du frontend (sérialisées en `snake_case`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// Document chiffré, aucun mot de passe fourni.
    PasswordRequired,
    /// Document chiffré, mot de passe fourni mais refusé.
    WrongPassword,
    /// Le fichier n'est pas un PDF lisible (ou le rendu demandé est démesuré).
    InvalidPdf,
    /// Document ou page inconnus.
    NotFound,
    /// Demande refusée dans l'état actuel (plage de pages invalide, dernière page…).
    InvalidOperation,
    /// Fichier introuvable ou inaccessible.
    Io,
    /// Défaillance interne (PDFium absente, verrou indisponible…).
    Internal,
    /// Une opération a paniqué : le document qu'elle modifiait a pu être fermé (Lot 13).
    Aborted,
}

impl AppError {
    /// Construit une erreur avec sa catégorie et un message déjà rédigé en français.
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        AppError {
            kind,
            message: message.into(),
        }
    }
}

impl From<CoreError> for AppError {
    fn from(e: CoreError) -> Self {
        let kind = match &e {
            CoreError::PasswordRequired => ErrorKind::PasswordRequired,
            CoreError::WrongPassword => ErrorKind::WrongPassword,
            CoreError::InvalidPdf(_) => ErrorKind::InvalidPdf,
            CoreError::PageOutOfRange { .. } => ErrorKind::NotFound,
            CoreError::Io(_) => ErrorKind::Io,
            CoreError::RenderTooLarge { .. } | CoreError::FileTooLarge { .. } => {
                ErrorKind::InvalidPdf
            }
            CoreError::NotFound { .. } => ErrorKind::NotFound,
            CoreError::InvalidPageSpec(_)
            | CoreError::InvalidOperation(_)
            | CoreError::Timeout { .. } => ErrorKind::InvalidOperation,
            CoreError::Cancelled => ErrorKind::Internal,
            CoreError::LibraryNotFound { .. }
            | CoreError::Pdfium(_)
            | CoreError::ServiceStopped => ErrorKind::Internal,
            CoreError::Aborted => ErrorKind::Aborted,
        };
        // Message complet, causes incluses (ex. « Erreur d'accès au fichier : Accès refusé »).
        let error = AppError::new(kind, mega_core::error::display_chain(&e));
        // Toute erreur du cœur passe par ici avant d'atteindre l'interface : c'est le seul
        // endroit où la noter dans le journal local (Lot 13).
        if !matches!(e, CoreError::Cancelled) {
            crate::diagnostic::record(&error);
        }
        error
    }
}

// \[Apprentissage\] Implémentation manuelle de `Serialize` : on choisit exactement la
// forme JSON (`{"kind": "...", "message": "..."}`) au lieu de laisser `derive` décider.
impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("kind", &self.kind)?;
        s.serialize_field("message", &self.message)?;
        s.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrong_password_has_its_own_kind() {
        let e = AppError::from(CoreError::WrongPassword);
        assert_eq!(e.kind, ErrorKind::WrongPassword);
        assert!(
            serde_json::to_string(&e)
                .unwrap()
                .contains(r#""kind":"wrong_password""#)
        );
    }

    #[test]
    fn serializes_as_kind_and_message() {
        let e = AppError::from(CoreError::PasswordRequired);
        let json = serde_json::to_string(&e).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"password_required","message":"Ce document est protégé par un mot de passe."}"#
        );
    }
}
