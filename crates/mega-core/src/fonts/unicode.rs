// SPDX-License-Identifier: GPL-3.0-or-later
//! Police Unicode incorporée : **DejaVu Sans** 2.37 (Lot 10, ADR-0014).
//!
//! Helvetica (Standard 14) n'est pas incorporée et ne connaît que l'encodage WinAnsi : 218
//! caractères, ni grec, ni cyrillique, ni « Ł ». Pour écrire un tel texte, il faut **embarquer**
//! une police dans le PDF. DejaVu Sans couvre le latin étendu, le grec et le cyrillique ; sa
//! licence (Bitstream Vera + domaine public) permet de la redistribuer avec un logiciel GPL.
//!
//! Ce module lit les tables de la police (correspondance caractère → glyphe, largeurs,
//! métriques) avec `ttf-parser`. L'analyse est faite **une seule fois** par processus.
//!
//! \[Apprentissage\] **`include_bytes!`** copie le fichier dans le binaire au moment de la
//! compilation : la police voyage avec le programme, aucun fichier à chercher à l'exécution.
//! Le résultat est un `&'static [u8]` : une tranche qui vit aussi longtemps que le programme,
//! ce qui permet de garder une analyse (`Face<'static>`) qui l'emprunte dans un `OnceLock`
//! (voir `docs/apprentissage/concepts/oncelock.md`).

// Branché sur les écritures au fil du Lot 10 (écrivain Type0, filigranes, signature) ; cet
// attribut temporaire disparaît quand tout est utilisé.
#![allow(dead_code)]

use std::sync::OnceLock;

use ttf_parser::{Face, GlyphId};

/// Le fichier de la police, tel que publié par le projet DejaVu (version 2.37).
pub(crate) static DEJAVU_SANS: &[u8] = include_bytes!("../../assets/fonts/DejaVuSans.ttf");

/// La police Unicode analysée : correspondances et métriques, en millièmes de la taille de
/// police (l'unité des largeurs PDF, §9.2.4).
pub(crate) struct UnicodeFont {
    face: Face<'static>,
}

impl UnicodeFont {
    /// La police embarquée, analysée au premier appel puis partagée.
    pub(crate) fn embedded() -> &'static UnicodeFont {
        static FONT: OnceLock<UnicodeFont> = OnceLock::new();
        FONT.get_or_init(|| {
            // La police est un fichier fixe du dépôt, compilé dans le binaire : son analyse
            // ne peut pas échouer à l'exécution si elle a réussi une fois (test
            // `embedded_font_maps_cyrillic_greek_and_latin_ext`).
            #[allow(clippy::expect_used)]
            let face = Face::parse(DEJAVU_SANS, 0).expect("police DejaVu Sans embarquée lisible");
            UnicodeFont { face }
        })
    }

    /// Le glyphe qui dessine `c`, s'il existe dans la police (le glyphe 0, « .notdef », ne
    /// compte pas : c'est le carré vide des caractères inconnus).
    pub(crate) fn glyph(&self, c: char) -> Option<u16> {
        self.face.glyph_index(c).map(|g| g.0).filter(|g| *g != 0)
    }

    /// Largeur d'avance du glyphe `gid`, en millièmes de la taille de police (arrondie).
    pub(crate) fn advance(&self, gid: u16) -> u16 {
        let units = self.face.glyph_hor_advance(GlyphId(gid)).unwrap_or(0);
        self.to_thousandths(i32::from(units)).max(0) as u16
    }

    /// Largeur d'avance, en points, de `text` à `size` points ; un caractère sans glyphe
    /// compte pour le glyphe 0 (c'est lui que le lecteur dessinera).
    pub(crate) fn text_width(&self, text: &str, size: f32) -> f32 {
        let thousandths: u32 = text
            .chars()
            .filter(|c| !matches!(c, '\n' | '\r'))
            .map(|c| u32::from(self.advance(self.glyph(c).unwrap_or(0))))
            .sum();
        thousandths as f32 * size / 1000.0
    }

    /// Caractères visibles de `text` sans glyphe, sans doublon, dans l'ordre d'apparition.
    pub(crate) fn missing(&self, text: &str) -> Vec<char> {
        let mut out: Vec<char> = Vec::new();
        for c in text.chars() {
            if c.is_control() || out.contains(&c) {
                continue;
            }
            if self.glyph(c).is_none() {
                out.push(c);
            }
        }
        out
    }

    /// Hauteur au-dessus de la ligne de base (`/Ascent`), en millièmes.
    pub(crate) fn ascent(&self) -> i32 {
        self.to_thousandths(i32::from(self.face.ascender()))
    }

    /// Profondeur sous la ligne de base (`/Descent`, négative), en millièmes.
    pub(crate) fn descent(&self) -> i32 {
        self.to_thousandths(i32::from(self.face.descender()))
    }

    /// Hauteur des capitales (`/CapHeight`), en millièmes.
    pub(crate) fn cap_height(&self) -> i32 {
        let units = self
            .face
            .capital_height()
            .unwrap_or_else(|| self.face.ascender());
        self.to_thousandths(i32::from(units))
    }

    /// Boîte englobante de tous les glyphes (`/FontBBox`), en millièmes.
    pub(crate) fn bbox(&self) -> [i32; 4] {
        let r = self.face.global_bounding_box();
        [r.x_min, r.y_min, r.x_max, r.y_max].map(|v| self.to_thousandths(i32::from(v)))
    }

    /// Unités de la police converties en millièmes de la taille (arrondi au plus proche).
    fn to_thousandths(&self, units: i32) -> i32 {
        let per_em = i32::from(self.face.units_per_em().max(1));
        // Division entière arrondie au plus proche, signe compris.
        let scaled = units * 1000;
        if scaled >= 0 {
            (scaled + per_em / 2) / per_em
        } else {
            (scaled - per_em / 2) / per_em
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::{missing_glyphs, needs_unicode};

    #[test]
    fn embedded_font_maps_cyrillic_greek_and_latin_ext() {
        let font = UnicodeFont::embedded();
        for c in ['Ж', 'Ω', 'Ł', 'é', 'A', ' '] {
            assert!(font.glyph(c).is_some(), "glyphe attendu pour {c}");
        }
    }

    #[test]
    fn cjk_is_missing() {
        assert_eq!(missing_glyphs("a你b好你\n"), vec!['你', '好']);
        assert!(missing_glyphs("Привет Ελλάδα Łódź").is_empty());
    }

    #[test]
    fn advance_of_space_matches_hmtx() {
        let font = UnicodeFont::embedded();
        let space = font.glyph(' ').unwrap();
        // DejaVu Sans : 651 unités sur 2048 → 317,87 millièmes, arrondi à 318.
        assert_eq!(font.advance(space), 318);
    }

    #[test]
    fn text_width_sums_advances_and_metrics_are_sane() {
        let font = UnicodeFont::embedded();
        let w = font.text_width("  ", 10.0);
        assert!((w - 6.36).abs() < 0.01, "{w}");
        assert!(
            font.ascent() > 700 && font.descent() < -200,
            "{} {}",
            font.ascent(),
            font.descent()
        );
        assert!(font.cap_height() > 600);
        let [x0, y0, x1, y1] = font.bbox();
        assert!(x0 < 0 && y0 < 0 && x1 > 1000 && y1 > 900);
    }

    #[test]
    fn needs_unicode_only_outside_win_ansi() {
        assert!(!needs_unicode("été €\n"));
        assert!(needs_unicode("Łódź"));
        assert!(needs_unicode("Привет"));
    }
}
