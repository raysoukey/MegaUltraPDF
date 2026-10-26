// SPDX-License-Identifier: GPL-3.0-or-later
//! Définition de la ligne de commande avec `clap` (mode « derive »).
//!
//! \[Apprentissage\] `#[derive(Parser)]` génère tout l'analyseur d'arguments à partir
//! de la structure : noms des options, valeurs par défaut, aide `--help`. Les
//! commentaires `///` deviennent les textes d'aide.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// MegaUltraPDF en ligne de commande.
#[derive(Debug, Parser)]
#[command(
    name = "mega",
    version,
    about = "Outil en ligne de commande de MegaUltraPDF"
)]
pub struct Cli {
    /// Sous-commande choisie par l'utilisateur.
    #[command(subcommand)]
    pub command: Command,
}

/// Les sous-commandes disponibles.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Affiche les informations d'un PDF (titre, nombre de pages, taille de la page 1).
    Info {
        /// Fichier PDF à inspecter.
        file: PathBuf,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Rend une page en image PNG.
    Render {
        /// Fichier PDF source.
        file: PathBuf,
        /// Numéro de page (à partir de 1).
        #[arg(long, default_value_t = 1)]
        page: u16,
        /// Facteur d'échelle (1.0 = 72 points par pouce).
        #[arg(long, default_value_t = 1.0)]
        scale: f32,
        /// Fichier PNG à écrire.
        #[arg(long)]
        out: PathBuf,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
}
