// SPDX-License-Identifier: GPL-3.0-or-later
//! Script de build : expose le triple de compilation au code du crate.
//!
//! Cargo fournit `TARGET` aux scripts de build mais pas au code compilé. On le
//! réexporte sous `MEGA_TARGET_TRIPLE` pour que `engine.rs` sache dans quel
//! sous-dossier de `vendor/pdfium/` chercher la bibliothèque.

fn main() {
    let target = std::env::var("TARGET").expect("Cargo définit toujours TARGET");
    println!("cargo:rustc-env=MEGA_TARGET_TRIPLE={target}");
    println!("cargo:rerun-if-changed=build.rs");
}
