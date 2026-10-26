// SPDX-License-Identifier: GPL-3.0-or-later
//! `mega` : outil en ligne de commande de MegaUltraPDF.
//!
//! Sert à la fois d'outil réel (inspection, rendu, plus tard fusion et scission)
//! et de banc d'essai pour `mega-core` sans passer par l'interface graphique.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod cli;
mod commands;

use clap::Parser;

use crate::cli::{Cli, Command};

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Info { file, password } => commands::info::run(&file, password.as_deref()),
        Command::Render {
            file,
            page,
            scale,
            out,
            password,
        } => commands::render::run(&file, page, scale, &out, password.as_deref()),
    };
    // \[Apprentissage\] `{e:#}` affiche la chaîne complète des causes d'une erreur
    // `anyhow` (« écriture de x : accès refusé »), là où `{e}` n'afficherait que la
    // dernière. On termine avec le code 1 pour que les scripts puissent détecter l'échec.
    if let Err(e) = result {
        eprintln!("Erreur : {e:#}");
        std::process::exit(1);
    }
}
