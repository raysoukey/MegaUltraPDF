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
