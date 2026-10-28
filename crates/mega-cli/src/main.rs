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

/// Taille de pile du thread qui exécute la commande.
const STACK_SIZE: usize = 8 * 1024 * 1024;

fn main() {
    // [Apprentissage] Sous Windows, le thread principal n'a que 1 Mo de pile ; en mode debug,
    // l'analyse des arguments par `clap` (une grosse `enum` de sous-commandes) la dépasse.
    // On exécute donc le programme dans un thread créé avec une pile de 8 Mo (celle de
    // Linux et macOS), et on attend sa fin avec `join`.
    let worker = std::thread::Builder::new()
        .stack_size(STACK_SIZE)
        .spawn(run);
    match worker.map(|handle| handle.join()) {
        Ok(Ok(())) => {}
        // Une panique a déjà été affichée par le thread ; un échec de création est rare.
        Ok(Err(_)) | Err(_) => std::process::exit(101),
    }
}

/// Le programme proprement dit : analyse des arguments, puis une sous-commande.
fn run() {
    let cli = Cli::parse();
    let force = cli.force;
    if let Err(e) = commands::preflight(&cli.command, force) {
        eprintln!("Erreur : {e:#}");
        std::process::exit(1);
    }
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
            pages,
            rect,
            out,
            password,
        } => commands::content::redact(&file, &pages, &rect, &out, password.as_deref()),
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
        Command::ExportImages {
            file,
            out,
            pages,
            dpi,
            jpeg,
            quality,
            prefix,
            password,
        } => commands::export::images(
            &file,
            &out,
            pages.as_deref(),
            dpi,
            jpeg,
            quality,
            prefix.as_deref(),
            force,
            password.as_deref(),
        ),
        Command::ExportText {
            file,
            out,
            pages,
            no_separators,
            password,
        } => commands::export::text(
            &file,
            &out,
            pages.as_deref(),
            !no_separators,
            password.as_deref(),
        ),
        Command::Meta {
            file,
            title,
            author,
            subject,
            keywords,
            out,
            password,
        } => commands::security::meta(
            &file,
            title.as_deref(),
            author.as_deref(),
            subject.as_deref(),
            keywords.as_deref(),
            out.as_deref(),
            password.as_deref(),
        ),
        Command::Protect {
            file,
            user,
            owner,
            no_print,
            no_copy,
            no_modify,
            no_annotate,
            out,
            password,
        } => commands::security::protect(
            &file,
            &user,
            &owner,
            no_print,
            no_copy,
            no_modify,
            no_annotate,
            &out,
            password.as_deref(),
        ),
        Command::Unprotect {
            file,
            password,
            out,
        } => commands::security::unprotect(&file, password.as_deref(), &out),
        Command::Optimize {
            file,
            quality,
            max_px,
            no_compress,
            out,
            password,
        } => commands::tools::optimize(
            &file,
            Some(quality),
            max_px,
            !no_compress,
            &out,
            password.as_deref(),
        ),
        Command::Attachments {
            file,
            extract,
            out,
            password,
        } => commands::tools::attachments(&file, extract, out.as_deref(), password.as_deref()),
        #[cfg(feature = "ocr")]
        Command::Ocr {
            file,
            lang,
            pages,
            tesseract,
            out,
            password,
        } => commands::ocr::run(
            &file,
            &out,
            &lang,
            pages.as_deref(),
            tesseract,
            password.as_deref(),
        ),
        #[cfg(feature = "signature")]
        Command::Sign {
            file,
            p12,
            reason,
            location,
            contact,
            out,
            password,
        } => commands::sign::sign(
            &file,
            &p12,
            &out,
            mega_core::digital_signature::SignOptions {
                reason,
                location,
                contact,
            },
            password.as_deref(),
        ),
        #[cfg(feature = "signature")]
        Command::Signatures { file, password } => commands::sign::list(&file, password.as_deref()),
        Command::Compare {
            a,
            b,
            page,
            page_b,
            scale,
            out,
            password,
            password_b,
        } => commands::tools::compare(
            &a,
            &b,
            page,
            page_b.unwrap_or(page),
            scale,
            &out,
            password.as_deref(),
            password_b.as_deref().or(password.as_deref()),
        ),
    };
    // \[Apprentissage\] `{e:#}` affiche la chaîne complète des causes d'une erreur
    // `anyhow` (« écriture de x : accès refusé »), là où `{e}` n'afficherait que la
    // dernière. On termine avec le code 1 pour que les scripts puissent détecter l'échec.
    if let Err(e) = result {
        eprintln!("Erreur : {e:#}");
        std::process::exit(1);
    }
}
