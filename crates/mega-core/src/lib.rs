// SPDX-License-Identifier: GPL-3.0-or-later
//! Cœur de MegaUltraPDF : toute la logique PDF, sans interface.
//!
//! Ce crate est le seul à parler à la bibliothèque PDFium. L'application de bureau
//! (`app/src-tauri`) et l'outil en ligne de commande (`mega-cli`) ne font qu'appeler
//! les types publics définis ici. Cette frontière permet de tester la logique sans
//! fenêtre et de changer de moteur plus tard sans toucher aux interfaces.

#![forbid(unsafe_code)]

/// Version du crate, lue depuis `Cargo.toml` au moment de la compilation.
///
/// `env!` est une macro évaluée à la compilation : si la variable n'existe pas,
/// la compilation échoue, ce qui vaut mieux qu'une chaîne vide à l'exécution.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
