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
//! - [`geometry`] : [`Rect`] (points PDF) et [`Rotation`].
//! - [`text`] : couche de texte ([`TextBox`]) et recherche ([`SearchHit`]).
//! - [`outline`] : signets ([`OutlineNode`]) ; [`links`] : liens de page ([`PageLink`]).
//! - [`properties`] : métadonnées et version ([`DocumentProperties`]).
//! - [`edit`] : commandes d'édition, historique annuler/rétablir, enregistrement atomique.
//! - [`pages`] : rotation, suppression, insertion, déplacement, extraction, fusion, scission.
//! - [`pagespec`] : analyse des plages de pages saisies (« 1-3,5 »).
//! - [`service`] : l'acteur PDFium (thread unique, file priorisée, cache, générations).
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

pub mod annotations;
pub mod cleanup;
pub mod content;
pub mod document;
pub mod edit;
pub mod engine;
pub mod error;
pub mod forms;
pub mod geometry;
pub mod links;
pub mod outline;
pub mod pages;
pub mod pagespec;
pub mod properties;
pub mod render;
pub mod service;
#[cfg(feature = "testing")]
pub mod testing;
pub mod text;

pub use annotations::{
    Annotation, AnnotationKind, AnnotationPatch, Color, Geometry, NewAnnotation, Style,
};
pub use content::{ObjectKind, PageObject};
pub use document::{Document, PageSize};
pub use edit::{Command, EditCommand, EditState, HistoryLimits};
pub use error::{CoreError, CoreResult};
pub use forms::{
    FieldKind, FieldOption, FieldValue, FormField, FormInfo, FormType, SignatureSource,
};
pub use geometry::{PageFrame, Rect, Rotation};
pub use links::{LinkTarget, PageLink};
pub use outline::OutlineNode;
pub use pagespec::PageSpec;
pub use properties::DocumentProperties;
pub use render::RenderedPage;
pub use text::{SearchHit, SearchOptions, TextBox};

/// Version du crate, lue depuis `Cargo.toml` au moment de la compilation.
///
/// `env!` est une macro évaluée à la compilation : si la variable n'existe pas,
/// la compilation échoue, ce qui vaut mieux qu'une chaîne vide à l'exécution.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
