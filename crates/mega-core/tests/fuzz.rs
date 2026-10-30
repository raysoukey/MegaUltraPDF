// SPDX-License-Identifier: GPL-3.0-or-later
//! Fuzzing court, à chaque `cargo test` (Lot 13, audit R9) : 200 variantes abîmées de vrais
//! fichiers passent par la chaîne d'un utilisateur sans paniquer. Les longues campagnes se
//! lancent avec `cargo xtask fuzz --iterations N` ; la graine de chaque cas est imprimée
//! avant son lancement, pour le rejouer si le processus s'arrête net (plantage natif).

use mega_core::testing::fuzz::{exercise, mutate};

/// Fichiers de départ : des structures variées (formulaire, annotations, signets,
/// signature, pièces jointes, xref hybride, révisions).
pub const SEEDS: [&str; 10] = [
    "simple.pdf",
    "form.pdf",
    "annotated.pdf",
    "outlined.pdf",
    "out-of-order.pdf",
    "attachments.pdf",
    "signed.pdf",
    "rotated.pdf",
    "robustesse/xref-hybride.pdf",
    "robustesse/revisions.pdf",
];

#[test]
fn two_hundred_damaged_files_never_panic() {
    let originals: Vec<Vec<u8>> = SEEDS
        .iter()
        .map(|name| {
            std::fs::read(format!(
                "{}/../../fixtures/{name}",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap()
        })
        .collect();
    let mut opened = 0;
    for seed in 0..200u64 {
        let original = &originals[seed as usize % originals.len()];
        eprintln!("graine {seed} ({})", SEEDS[seed as usize % SEEDS.len()]);
        opened += usize::from(exercise(mutate(original, seed)));
    }
    // Garde-fou du test lui-même : assez de fichiers abîmés doivent encore s'ouvrir pour
    // exercer le rendu, le texte et l'enregistrement (PDFium répare beaucoup).
    assert!(opened >= 50, "seulement {opened} fichiers ouverts sur 200");
}

/// La même graine redonne le même fichier : un échec se rejoue à l'identique.
#[test]
fn mutations_are_reproducible() {
    let bytes = mega_core::testing::sample_pdf(1, "graine");
    assert_eq!(mutate(&bytes, 42), mutate(&bytes, 42));
    assert_ne!(mutate(&bytes, 42), mutate(&bytes, 43));
}
