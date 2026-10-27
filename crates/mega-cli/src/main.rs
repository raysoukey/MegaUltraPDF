// SPDX-License-Identifier: GPL-3.0-or-later
//! `mega` : outil en ligne de commande de MegaUltraPDF.
//!
//! Sert à la fois d'outil réel (inspection, rendu, plus tard fusion et scission)
//! et de banc d'essai pour `mega-core` sans passer par l'interface graphique.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
// Politique d'erreurs (ADR-0005) : pas de panique dans le code de production.
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
        Command::Merge { files, out } => commands::merge::run(&files, &out),
        Command::Split {
            file,
            ranges,
            every,
            bookmarks,
            out,
            prefix,
            force,
            password,
        } => {
            use mega_core::pages::SplitMode;
            let mode = match (ranges, every, bookmarks) {
                (Some(spec), _, _) => SplitMode::Ranges { spec },
                (None, Some(n), _) => SplitMode::Every { n },
                (None, None, true) => SplitMode::Bookmarks,
                (None, None, false) => SplitMode::Every { n: 1 },
            };
            commands::split::run(
                &file,
                mode,
                &out,
                prefix.as_deref(),
                force,
                password.as_deref(),
            )
        }
        Command::Rotate {
            file,
            pages,
            degrees,
            out,
            password,
        } => commands::rotate::run(&file, &pages, degrees, &out, password.as_deref()),
        Command::Annots {
            file,
            page,
            password,
        } => commands::annots::run(&file, page, password.as_deref()),
        Command::Fields { file, password } => commands::fields::run(&file, password.as_deref()),
        Command::Flatten {
            file,
            out,
            password,
        } => commands::flatten::run(&file, &out, password.as_deref()),
        Command::Extract {
            file,
            pages,
            out,
            password,
        } => commands::extract::run(&file, &pages, &out, password.as_deref()),
        Command::Objects {
            file,
            page,
            password,
        } => commands::content::objects(&file, page, password.as_deref()),
        Command::Redact {
            file,
            page,
            rect,
            out,
            password,
        } => commands::content::redact(&file, page, &rect, &out, password.as_deref()),
        Command::Watermark {
            file,
            text,
            size,
            opacity,
            angle,
            pages,
            out,
            password,
        } => commands::content::watermark(
            &file,
            &text,
            size,
            opacity,
            angle,
            pages.as_deref(),
            &out,
            password.as_deref(),
        ),
        Command::Number {
            file,
            template,
            corner,
            margin,
            size,
            start,
            pages,
            out,
            password,
        } => commands::content::number(
            &file,
            &template,
            &corner,
            margin,
            size,
            start,
            pages.as_deref(),
            &out,
            password.as_deref(),
        ),
        Command::Crop {
            file,
            margins,
            pages,
            out,
            password,
        } => commands::content::crop(&file, &margins, pages.as_deref(), &out, password.as_deref()),
    };
    // \[Apprentissage\] `{e:#}` affiche la chaîne complète des causes d'une erreur
    // `anyhow` (« écriture de x : accès refusé »), là où `{e}` n'afficherait que la
    // dernière. On termine avec le code 1 pour que les scripts puissent détecter l'échec.
    if let Err(e) = result {
        eprintln!("Erreur : {e:#}");
        std::process::exit(1);
    }
}
