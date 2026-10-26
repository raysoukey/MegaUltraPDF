// SPDX-License-Identifier: GPL-3.0-or-later
//! `mega` : outil en ligne de commande de MegaUltraPDF.

#![forbid(unsafe_code)]

fn main() {
    println!("mega {}", mega_core::VERSION);
}
