// SPDX-License-Identifier: GPL-3.0-or-later
//! Une fonction `run` par sous-commande.

pub mod annots;
pub mod content;
pub mod export;
pub mod extract;
pub mod fields;
pub mod flatten;
pub mod info;
pub mod merge;
#[cfg(feature = "ocr")]
pub mod ocr;
pub mod render;
pub mod rotate;
pub mod security;
#[cfg(feature = "signature")]
pub mod sign;
pub mod split;
pub mod tools;

use std::path::Path;

use crate::cli::Command;

/// Contrôles faits avant toute commande : ne jamais écraser un fichier sans `--force` (même
/// quand la sortie est le fichier source : une erreur au milieu ne laisserait rien), et
/// prévenir quand le fichier réécrit perdra ses signatures numériques.
pub fn preflight(command: &Command, force: bool) -> anyhow::Result<()> {
    if let Some(out) = command.output_file() {
        anyhow::ensure!(
            force || !out.exists(),
            "{} existe déjà : ajoutez --force pour l'écraser",
            out.display()
        );
    }
    if let Some(input) = command.rewritten_input() {
        warn_if_signed(input);
    }
    Ok(())
}

/// Avertit (sur la sortie d'erreur) si `input` porte une signature numérique.
#[cfg(feature = "signature")]
fn warn_if_signed(input: &Path) {
    // Lecture brute : sans `/ByteRange`, `signatures` n'analyse même pas le fichier. Un
    // fichier chiffré ou illisible n'arrête rien ici : la commande donnera sa propre erreur.
    let Ok(bytes) = std::fs::read(input) else {
        return;
    };
    if mega_core::digital_signature::signatures(&bytes).is_ok_and(|s| !s.is_empty()) {
        eprintln!(
            "Attention : {} porte une signature numérique ; elle ne sera plus valide dans le \
             fichier produit.",
            input.display()
        );
    }
}

/// Sans la feature `signature`, rien à vérifier.
#[cfg(not(feature = "signature"))]
fn warn_if_signed(_input: &Path) {}
