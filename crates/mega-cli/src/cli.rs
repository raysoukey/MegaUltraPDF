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
    /// Fusionne plusieurs PDF, dans l'ordre donné, en un nouveau fichier.
    Merge {
        /// Fichiers à fusionner (au moins un).
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Fichier PDF à écrire.
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Scinde un PDF en plusieurs fichiers `<préfixe>-N.pdf`.
    Split {
        /// Fichier PDF source.
        file: PathBuf,
        /// Plages explicites, une partie par plage (ex. « 1-2,3-5 »).
        #[arg(long, conflicts_with_all = ["every", "bookmarks"])]
        ranges: Option<String>,
        /// Une partie toutes les N pages (N ≥ 1).
        #[arg(long, conflicts_with = "bookmarks", value_parser = clap::value_parser!(u16).range(1..))]
        every: Option<u16>,
        /// Une partie par signet de premier niveau.
        #[arg(long)]
        bookmarks: bool,
        /// Dossier de sortie (créé si besoin).
        #[arg(short, long)]
        out: PathBuf,
        /// Préfixe des fichiers produits (défaut : nom du fichier source).
        #[arg(long)]
        prefix: Option<String>,
        /// Écraser les fichiers de sortie existants.
        #[arg(long)]
        force: bool,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Pivote des pages (rotation enregistrée dans le fichier) vers un nouveau fichier.
    Rotate {
        /// Fichier PDF source.
        file: PathBuf,
        /// Pages à pivoter (ex. « 1,3-4 »).
        #[arg(long)]
        pages: String,
        /// Angle horaire : 90, 180 ou 270.
        #[arg(long)]
        degrees: u16,
        /// Fichier PDF à écrire.
        #[arg(short, long)]
        out: PathBuf,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Liste les annotations d'un PDF.
    Annots {
        /// Fichier PDF à inspecter.
        file: PathBuf,
        /// Page (à partir de 1) ; toutes si absent.
        #[arg(long)]
        page: Option<u16>,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Copie des pages dans un nouveau fichier.
    Extract {
        /// Fichier PDF source.
        file: PathBuf,
        /// Pages à extraire (ex. « 2-3 »), dans l'ordre donné.
        #[arg(long)]
        pages: String,
        /// Fichier PDF à écrire.
        #[arg(short, long)]
        out: PathBuf,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
}
