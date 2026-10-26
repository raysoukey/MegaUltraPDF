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
    PasswordRequired,
    InvalidPdf,
    NotFound,
    Io,
    Internal,
}

impl AppError {
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
            CoreError::InvalidPdf(_) => ErrorKind::InvalidPdf,
            CoreError::PageOutOfRange { .. } => ErrorKind::NotFound,
            CoreError::Io(_) => ErrorKind::Io,
            CoreError::LibraryNotFound { .. } | CoreError::Pdfium(_) => ErrorKind::Internal,
        };
        AppError::new(kind, e.to_string())
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
    fn serializes_as_kind_and_message() {
        let e = AppError::from(CoreError::PasswordRequired);
        let json = serde_json::to_string(&e).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"password_required","message":"Ce document est protégé par un mot de passe."}"#
        );
    }
}
