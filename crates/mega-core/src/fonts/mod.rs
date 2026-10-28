// SPDX-License-Identifier: GPL-3.0-or-later
//! Largeurs des glyphes d'Helvetica (police Standard 14) en encodage WinAnsi.
//!
//! Une police Standard 14 n'est pas incorporée : le lecteur la fournit, avec ses propres
//! métriques. Écrire les largeurs (`/Widths`) dans nos dictionnaires de police et mesurer nos
//! textes avec elles garantit que tous les lecteurs placent les caractères au même endroit
//! (Lot 9, B24 et C46). Valeurs : fichier AFM d'Adobe « Helvetica », en millièmes de la
//! taille de police.
//!
//! \[Apprentissage\] Une **table constante** (`const [u16; 224]`) est rangée dans le binaire
//! lui-même : aucune allocation, aucun fichier à lire, et l'indexation est vérifiée par le
//! compilateur pour la taille du tableau.

use lopdf::Object;

pub(crate) mod unicode;

/// Caractères **visibles** de `text` absents de la police Unicode incorporée (DejaVu Sans),
/// sans doublon, dans l'ordre d'apparition. Sert à refuser clairement un texte qu'aucune de
/// nos polices ne sait dessiner (CJK, par exemple) au lieu d'écrire des carrés vides.
pub fn missing_glyphs(text: &str) -> Vec<char> {
    unicode::UnicodeFont::embedded().missing(text)
}

/// Vrai si `text` contient un caractère que l'encodage WinAnsi d'Helvetica ne connaît pas :
/// il faut alors la police Unicode (Lot 10). Les retours à la ligne ne comptent pas.
pub(crate) fn needs_unicode(text: &str) -> bool {
    text.chars()
        .any(|c| !matches!(c, '\n' | '\r') && crate::content::win_ansi_byte(c).is_none())
}

/// Premier code de la table (l'espace).
pub(crate) const FIRST_CHAR: u8 = 32;

/// Largeurs des codes 32 à 255. Les codes que WinAnsi laisse libres (127, 129, 141, 143,
/// 144, 157) s'affichent comme une puce (ISO 32000-1, annexe D, note 6) : largeur 350.
#[rustfmt::skip]
const HELVETICA_WIDTHS: [u16; 224] = [
    // 32-63 : espace ! " # $ % & ' ( ) * + , - . / 0-9 : ; < = > ?
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278,
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556,
    // 64-95 : @ A-Z [ \ ] ^ _
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778,
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556,
    // 96-127 : ` a-z { | } ~ (127 : puce)
    333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556,
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584, 350,
    // 128-159 : € (puce) ‚ ƒ „ … † ‡ ˆ ‰ Š ‹ Œ (puce) Ž (puce) (puce) ‘ ’ “ ” • – — ˜ ™ š › œ (puce) ž Ÿ
    556, 350, 222, 556, 333, 1000, 556, 556, 333, 1000, 667, 333, 1000, 350, 611, 350,
    350, 222, 222, 333, 333, 350, 556, 1000, 333, 1000, 500, 333, 944, 350, 500, 667,
    // 160-191 : espace insécable ¡ ¢ £ ¤ ¥ ¦ § ¨ © ª « ¬ trait d'union conditionnel ® ¯
    //           ° ± ² ³ ´ µ ¶ · ¸ ¹ º » ¼ ½ ¾ ¿
    278, 333, 556, 556, 556, 556, 260, 556, 333, 737, 370, 556, 584, 333, 737, 333,
    400, 584, 333, 333, 333, 556, 537, 278, 333, 333, 365, 556, 834, 834, 834, 611,
    // 192-223 : À Á Â Ã Ä Å Æ Ç È É Ê Ë Ì Í Î Ï Ð Ñ Ò Ó Ô Õ Ö × Ø Ù Ú Û Ü Ý Þ ß
    667, 667, 667, 667, 667, 667, 1000, 722, 667, 667, 667, 667, 278, 278, 278, 278,
    722, 722, 778, 778, 778, 778, 778, 584, 778, 722, 722, 722, 722, 667, 667, 611,
    // 224-255 : à á â ã ä å æ ç è é ê ë ì í î ï ð ñ ò ó ô õ ö ÷ ø ù ú û ü ý þ ÿ
    556, 556, 556, 556, 556, 556, 889, 500, 556, 556, 556, 556, 278, 278, 278, 278,
    556, 556, 556, 556, 556, 556, 556, 584, 611, 556, 556, 556, 556, 500, 556, 500,
];

/// Largeur (millièmes de la taille) du caractère WinAnsi `byte` ; 0 pour un code de contrôle.
pub(crate) fn helvetica_width(byte: u8) -> u16 {
    byte.checked_sub(FIRST_CHAR)
        .and_then(|i| HELVETICA_WIDTHS.get(usize::from(i)))
        .copied()
        .unwrap_or(0)
}

/// Largeur d'avance, en points, de `bytes` (texte déjà encodé en WinAnsi) à `size` points.
pub(crate) fn text_width(bytes: &[u8], size: f32) -> f32 {
    let thousandths: u32 = bytes.iter().map(|b| u32::from(helvetica_width(*b))).sum();
    thousandths as f32 * size / 1000.0
}

/// Les clés `/FirstChar`, `/LastChar` et `/Widths` d'un dictionnaire de police Helvetica
/// WinAnsi, prêtes à y être ajoutées.
pub(crate) fn widths_entries() -> [(&'static str, Object); 3] {
    [
        ("FirstChar", Object::Integer(i64::from(FIRST_CHAR))),
        ("LastChar", Object::Integer(255)),
        (
            "Widths",
            Object::Array(
                HELVETICA_WIDTHS
                    .iter()
                    .map(|w| Object::Integer(i64::from(*w)))
                    .collect(),
            ),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helvetica_widths_match_the_afm() {
        assert_eq!(helvetica_width(b' '), 278);
        assert_eq!(helvetica_width(b'A'), 667);
        assert_eq!(helvetica_width(b'i'), 222);
        assert_eq!(helvetica_width(b'W'), 944);
        assert_eq!(helvetica_width(0xE9), 556, "é");
        assert_eq!(helvetica_width(0x80), 556, "€");
        assert_eq!(helvetica_width(0x95), 350, "•");
        assert_eq!(helvetica_width(0x0A), 0, "contrôle");
    }
}
