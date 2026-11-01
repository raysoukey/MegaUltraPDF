// SPDX-License-Identifier: GPL-3.0-or-later
//! Avertissements du cœur destinés au **journal de diagnostic** de l'application.
//!
//! Certaines difficultés ne doivent pas faire échouer l'opération en cours, mais méritent une
//! trace : par exemple un XMP que PDFium a laissé en clair et que nous n'avons pas pu
//! rechiffrer (l'enregistrement a lieu quand même). `mega-core` ne connaît pas le journal de
//! l'application (`app/src-tauri/src/diagnostic.rs`, qui dépend de lui et non l'inverse) :
//! l'application installe donc au démarrage une fonction de réception par
//! [`set_warning_sink`]. Sans elle (CLI, tests), l'avertissement va sur la sortie d'erreur.
//!
//! Le message ne cite jamais le contenu d'un document ; l'application le masque de toute
//! façon (`sanitize`) avant de l'écrire.
//!
//! \[Apprentissage\] **`fn(&str)`, un pointeur de fonction** : un type qui désigne n'importe
//! quelle fonction ordinaire de cette signature. Contrairement à une fermeture (`Fn`), il ne
//! capture rien : il tient dans une `static` sans `Box` ni durée de vie à expliquer.

use std::sync::OnceLock;

/// La fonction qui reçoit les avertissements, posée une seule fois par l'application.
static SINK: OnceLock<fn(&str)> = OnceLock::new();

/// Installe la fonction qui recevra les avertissements du cœur (une seule fois, au démarrage
/// de l'application) : c'est ainsi qu'ils rejoignent son journal de diagnostic. `false` si une
/// fonction était déjà installée (elle reste en place).
pub fn set_warning_sink(sink: fn(&str)) -> bool {
    SINK.set(sink).is_ok()
}

/// Transmet un avertissement à la fonction installée, sinon l'écrit sur la sortie d'erreur.
pub(crate) fn warn(message: &str) {
    match SINK.get() {
        Some(sink) => sink(message),
        None => eprintln!("avertissement : {message}"),
    }
}
