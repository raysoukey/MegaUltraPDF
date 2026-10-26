// SPDX-License-Identifier: GPL-3.0-or-later
//! Cœur de MegaUltraPDF : toute la logique PDF, sans interface.
//!
//! Ce crate est le seul à parler à la bibliothèque PDFium. L'application de bureau
//! (`app/src-tauri`) et l'outil en ligne de commande (`mega-cli`) ne font qu'appeler
//! les types publics définis ici. Cette frontière permet de tester la logique sans
//! fenêtre et de changer de moteur plus tard sans toucher aux interfaces.
//!
//! Organisation des modules :
//! - [`error`] : le type d'erreur unique du crate, [`CoreError`].
//! - [`engine`] : localisation et initialisation (une seule fois) de PDFium.
//! - [`document`] : [`Document`], un PDF ouvert, et ses métadonnées.
//! - [`render`] : rendu d'une page en pixels ([`RenderedPage`]).
//! - `testing` : génération de PDF d'exemple (feature `testing`).

#![forbid(unsafe_code)]
// Politique d'erreurs (ADR-0005) : pas de panique dans le code de production. Les tests
// (`cfg(test)`) en sont exemptés ; un `expect` justifié porte un `#[allow]` commenté.
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]

pub mod document;
pub mod engine;
pub mod error;
pub mod render;
#[cfg(feature = "testing")]
pub mod testing;

pub use document::{Document, PageSize};
pub use error::{CoreError, CoreResult};
pub use render::RenderedPage;

/// Version du crate, lue depuis `Cargo.toml` au moment de la compilation.
///
/// `env!` est une macro évaluée à la compilation : si la variable n'existe pas,
/// la compilation échoue, ce qui vaut mieux qu'une chaîne vide à l'exécution.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
