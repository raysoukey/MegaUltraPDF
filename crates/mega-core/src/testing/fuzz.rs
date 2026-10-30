// SPDX-License-Identifier: GPL-3.0-or-later
//! Fuzzing **déterministe** sans dépendance (Lot 13, audit R9).
//!
//! On part de vrais fichiers PDF, on les abîme de façon pseudo-aléatoire (bits inversés,
//! troncature, bloc dupliqué, mot-clé PDF glissé au hasard) et on fait passer chaque
//! variante par la chaîne d'un utilisateur ([`exercise`]). Une panique, un plantage ou un
//! blocage est un bug ; une erreur typée est la réponse attendue. Chaque cas est désigné par
//! sa **graine** : la même graine redonne exactement le même fichier, pour rejouer un échec.
//!
//! \[Apprentissage\] **Générateur pseudo-aléatoire** : `xorshift64` mélange les bits d'un
//! entier par trois décalages et « ou exclusif ». Ce n'est pas de la cryptographie, mais c'est
//! rapide, reproductible et suffisant pour choisir des positions dans un fichier. Les outils
//! comme `cargo fuzz` sont plus malins (ils suivent la couverture du code) mais demandent un
//! compilateur nightly et ne tournent pas partout ; celui-ci tourne dans `cargo test`.

use crate::document::Document;
use crate::text::SearchOptions;

/// Générateur `xorshift64` : la suite de nombres est entièrement fixée par la graine.
pub struct XorShift(u64);

impl XorShift {
    /// Un générateur à partir de `seed`. La graine est d'abord bien mélangée (étape
    /// « SplitMix64 ») : deux graines voisines (42 et 43) donnent des suites sans rapport. Un
    /// état nul est remplacé, car xorshift y resterait bloqué.
    pub fn new(seed: u64) -> Self {
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        XorShift(if z == 0 { 1 } else { z })
    }

    /// Le nombre suivant.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Un entier dans `0..bound` (`bound` > 0).
    pub fn below(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound.max(1) as u64) as usize
    }
}

/// Morceaux de syntaxe PDF glissés au hasard : ils visent l'analyseur là où il est fragile.
const TOKENS: [&[u8]; 10] = [
    b"endobj",
    b"stream\n",
    b"endstream",
    b" 0 R ",
    b"<<",
    b">>",
    b"[",
    b"99999999999999999999",
    b"/Length 999999999",
    b"%%EOF",
];

/// `bytes` abîmé par 1 à 4 mutations choisies par la graine `seed`.
pub fn mutate(bytes: &[u8], seed: u64) -> Vec<u8> {
    let mut rng = XorShift::new(seed);
    let mut out = bytes.to_vec();
    for _ in 0..=rng.below(4) {
        if out.is_empty() {
            break;
        }
        let at = rng.below(out.len());
        match rng.below(4) {
            0 => {
                for _ in 0..=rng.below(8) {
                    let k = rng.below(out.len());
                    out[k] ^= 1 << rng.below(8);
                }
            }
            1 => out.truncate(at.max(1)),
            2 => {
                let len = rng.below(256).min(out.len() - at);
                let chunk = out[at..at + len].to_vec();
                let to = rng.below(out.len());
                out.splice(to..to, chunk);
            }
            _ => {
                let token = TOKENS[rng.below(TOKENS.len())];
                out.splice(at..at, token.iter().copied());
            }
        }
    }
    out
}

/// La chaîne d'un utilisateur sur des octets quelconques : ouvrir, puis, si le fichier est
/// accepté, rendre et lire chaque page (au plus trois), chercher, réécrire et nettoyer pour
/// l'enregistrement. Les erreurs sont ignorées : seul compte de ne pas paniquer ni planter.
/// Rend vrai si le fichier a été accepté (pour mesurer la part des cas qui vont loin).
pub fn exercise(bytes: Vec<u8>) -> bool {
    let Ok(doc) = Document::from_bytes(bytes, None) else {
        return false;
    };
    for page in 0..doc.page_count().min(3) {
        let _ = doc.render_page(page, 0.3);
        let _ = doc.text_boxes(page);
        let _ = doc.page_text(page);
        let _ = doc.search_page(page, "a", &SearchOptions::default());
    }
    let _ = doc.properties();
    let _ = doc.outline();
    if let Ok(saved) = doc.to_bytes() {
        let _ = crate::cleanup::before_save(&saved, None);
    }
    true
}
