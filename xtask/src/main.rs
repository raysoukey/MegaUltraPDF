// SPDX-License-Identifier: GPL-3.0-or-later
//! Tâches de développement, lancées avec `cargo xtask <commande>`.
//!
//! Le motif « xtask » est une convention de l'écosystème Rust : plutôt qu'un
//! Makefile ou des scripts shell, les tâches de dev sont un petit binaire Rust
//! du workspace, donc portables et testables.

#![forbid(unsafe_code)]

fn main() -> anyhow::Result<()> {
    println!("xtask: no command yet");
    Ok(())
}
