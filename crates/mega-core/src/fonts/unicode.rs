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

use std::collections::BTreeMap;
use std::sync::OnceLock;

use lopdf::{Dictionary, Document as LoDocument, Object, ObjectId, Stream, StringFormat};
use ttf_parser::{Face, GlyphId};

use crate::error::{CoreError, CoreResult};

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

/// Écrit **une** police Unicode dans un PDF (passe `lopdf`) pour les textes d'une même
/// écriture : filigrane, numéros, couche OCR, apparence de signature.
///
/// La police est de type `Type0` (police composite, §9.7) avec l'encodage `Identity-H` : chaque
/// caractère est codé sur **deux octets**, un identifiant de caractère (CID). Nous attribuons
/// les CID 1, 2, 3… dans l'ordre où les caractères apparaissent ; trois tables font le lien :
/// - `/CIDToGIDMap` : CID → glyphe du sous-ensemble incorporé (0 si la police n'a pas ce
///   caractère : c'est permis pour un texte **invisible**, l'appelant refuse sinon) ;
/// - `/W` : CID → largeur, pour que tous les lecteurs placent les caractères pareil ;
/// - `/ToUnicode` : CID → caractère Unicode, pour la recherche et le copier-coller.
///
/// \[Apprentissage\] Le **constructeur par accumulation** : on appelle `encode` autant de fois
/// que nécessaire (chaque appel enrichit la table des CID), puis `write` **consomme** l'écrivain
/// (`self` par valeur) : impossible de l'utiliser encore après avoir écrit la police.
#[derive(Debug, Default)]
pub(crate) struct Type0Writer {
    /// Caractères dans l'ordre des CID (le CID de `chars[i]` est `i + 1`).
    chars: Vec<char>,
    /// CID de chaque caractère déjà vu.
    cids: BTreeMap<char, u16>,
}

impl Type0Writer {
    /// Un écrivain vide.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Le texte en chaîne hexadécimale `<…>` de CID sur deux octets, prête pour `Tj`. Les
    /// caractères de contrôle (retours à la ligne) sont ignorés.
    pub(crate) fn encode(&mut self, text: &str) -> String {
        let mut out = String::with_capacity(text.len() * 4 + 2);
        out.push('<');
        for c in text.chars().filter(|c| !c.is_control()) {
            let cid = match self.cids.get(&c) {
                Some(cid) => *cid,
                None => {
                    self.chars.push(c);
                    // Au plus 65 535 caractères distincts : bien au-delà de nos textes
                    // (200 caractères pour un filigrane) ; au-delà, le dernier CID est réutilisé.
                    let cid = u16::try_from(self.chars.len()).unwrap_or(u16::MAX);
                    self.cids.insert(c, cid);
                    cid
                }
            };
            out.push_str(&format!("{cid:04X}"));
        }
        out.push('>');
        out
    }

    /// Vrai si aucun texte n'a été encodé (pas de police à écrire).
    pub(crate) fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    /// Écrit la police (dictionnaire `Type0` et ses descendants) dans `doc` et rend la
    /// référence à poser dans les ressources `/Font` des pages.
    pub(crate) fn write(self, doc: &mut LoDocument) -> CoreResult<ObjectId> {
        let font = UnicodeFont::embedded();
        // Glyphes d'origine, dans l'ordre des CID ; le sous-ensemble les renumérote 1, 2, …
        let gids: Vec<u16> = self
            .chars
            .iter()
            .map(|c| font.glyph(*c).unwrap_or(0))
            .collect();
        let mut remapper = subsetter::GlyphRemapper::new();
        for gid in &gids {
            remapper.remap(*gid);
        }
        let program = subsetter::subset(DEJAVU_SANS, 0, &remapper).map_err(|e| {
            CoreError::InvalidOperation(format!("sous-ensemble de la police impossible : {e}"))
        })?;

        // `/CIDToGIDMap` : deux octets (gros-boutiste) par CID, du CID 0 au dernier.
        let mut map = vec![0u8; 2 * (gids.len() + 1)];
        for (i, gid) in gids.iter().enumerate() {
            let new = remapper.get(*gid).unwrap_or(0);
            map[2 * (i + 1)..2 * (i + 2)].copy_from_slice(&new.to_be_bytes());
        }
        let map_id = doc.add_object(compressed(Dictionary::new(), map));

        let length1 = i64::try_from(program.len()).unwrap_or(i64::MAX);
        let file_id = doc.add_object(compressed(
            Dictionary::from_iter(vec![("Length1", Object::Integer(length1))]),
            program,
        ));
        let base_font = format!("{}+DejaVuSans", subset_tag(&self.chars));
        let [x0, y0, x1, y1] = font.bbox();
        let descriptor_id = doc.add_object(Dictionary::from_iter(vec![
            ("Type", Object::Name(b"FontDescriptor".to_vec())),
            ("FontName", Object::Name(base_font.clone().into_bytes())),
            // 32 : police non symbolique (jeu de caractères latin standard, §9.8.2).
            ("Flags", Object::Integer(32)),
            (
                "FontBBox",
                Object::Array(
                    [x0, y0, x1, y1]
                        .iter()
                        .map(|v| Object::Integer(i64::from(*v)))
                        .collect(),
                ),
            ),
            ("ItalicAngle", Object::Integer(0)),
            ("Ascent", Object::Integer(i64::from(font.ascent()))),
            ("Descent", Object::Integer(i64::from(font.descent()))),
            ("CapHeight", Object::Integer(i64::from(font.cap_height()))),
            ("StemV", Object::Integer(80)),
            ("FontFile2", Object::Reference(file_id)),
        ]));

        // `/W [1 [w1 w2 …]]` : largeurs des CID consécutifs à partir de 1.
        let widths: Vec<Object> = gids
            .iter()
            .map(|gid| Object::Integer(i64::from(font.advance(*gid))))
            .collect();
        let cid_font_id = doc.add_object(Dictionary::from_iter(vec![
            ("Type", Object::Name(b"Font".to_vec())),
            ("Subtype", Object::Name(b"CIDFontType2".to_vec())),
            ("BaseFont", Object::Name(base_font.clone().into_bytes())),
            (
                "CIDSystemInfo",
                Object::Dictionary(Dictionary::from_iter(vec![
                    (
                        "Registry",
                        Object::String(b"Adobe".to_vec(), StringFormat::Literal),
                    ),
                    (
                        "Ordering",
                        Object::String(b"Identity".to_vec(), StringFormat::Literal),
                    ),
                    ("Supplement", Object::Integer(0)),
                ])),
            ),
            ("FontDescriptor", Object::Reference(descriptor_id)),
            ("DW", Object::Integer(i64::from(font.advance(0)))),
            (
                "W",
                Object::Array(vec![Object::Integer(1), Object::Array(widths)]),
            ),
            ("CIDToGIDMap", Object::Reference(map_id)),
        ]));

        let to_unicode_id = doc.add_object(compressed(
            Dictionary::new(),
            to_unicode_cmap(&self.chars).into_bytes(),
        ));
        Ok(doc.add_object(Dictionary::from_iter(vec![
            ("Type", Object::Name(b"Font".to_vec())),
            ("Subtype", Object::Name(b"Type0".to_vec())),
            ("BaseFont", Object::Name(base_font.into_bytes())),
            ("Encoding", Object::Name(b"Identity-H".to_vec())),
            (
                "DescendantFonts",
                Object::Array(vec![Object::Reference(cid_font_id)]),
            ),
            ("ToUnicode", Object::Reference(to_unicode_id)),
        ])))
    }
}

/// Un flux comprimé (Flate) ; s'il ne se comprime pas, il reste tel quel.
fn compressed(dict: Dictionary, content: Vec<u8>) -> Stream {
    let mut stream = Stream::new(dict, content);
    // `compress` ne fait rien si le résultat serait plus gros ; une erreur laisse le flux brut.
    let _ = stream.compress();
    stream
}

/// Étiquette de sous-ensemble (six majuscules, §9.6.4) : déterministe, tirée des caractères,
/// pour que deux sous-ensembles différents d'un même fichier ne portent pas le même nom.
fn subset_tag(chars: &[char]) -> String {
    // FNV-1a 32 bits : un hachage simple, suffisant pour distinguer des noms.
    let mut hash: u32 = 0x811c_9dc5;
    for c in chars {
        for byte in u32::from(*c).to_le_bytes() {
            hash ^= u32::from(byte);
            hash = hash.wrapping_mul(0x0100_0193);
        }
    }
    (0..6)
        .map(|i| {
            let letter = (hash >> (i * 5)) % 26;
            char::from(b'A' + letter as u8)
        })
        .collect()
}

/// CMap `/ToUnicode` (§9.10.3) : un `bfchar` par CID, valeur en UTF-16BE (paire de
/// substitution au-delà du plan multilingue de base), par blocs de 100 (limite usuelle).
fn to_unicode_cmap(chars: &[char]) -> String {
    let mut out = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    for (block, slice) in chars.chunks(100).enumerate() {
        out.push_str(&format!("{} beginbfchar\n", slice.len()));
        for (i, c) in slice.iter().enumerate() {
            let cid = block * 100 + i + 1;
            let mut units = [0u16; 2];
            let utf16: String = c
                .encode_utf16(&mut units)
                .iter()
                .map(|u| format!("{u:04X}"))
                .collect();
            out.push_str(&format!("<{cid:04X}> <{utf16}>\n"));
        }
        out.push_str("endbfchar\n");
    }
    out.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::fonts::{missing_glyphs, needs_unicode};

    /// Une page A4 portant `text` en police Unicode (écrite par `Type0Writer`) ; rend les
    /// octets du fichier et la taille de l'objet police (sous-ensemble compris).
    fn page_with_unicode_text(text: &str) -> (Vec<u8>, usize) {
        let base = crate::testing::blank_pdf(595.0, 842.0);
        let mut doc = LoDocument::load_mem(&base).unwrap();
        let before = doc.objects.len();
        let mut writer = Type0Writer::new();
        let hex = writer.encode(text);
        let font_id = writer.write(&mut doc).unwrap();
        let mut font_bytes = 0;
        for (id, obj) in &doc.objects {
            if id.0 as usize > before
                && let Ok(s) = obj.as_stream()
            {
                font_bytes += s.content.len();
            }
        }
        let page_id = *doc.get_pages().get(&1).unwrap();
        let content = doc.add_object(Stream::new(
            Dictionary::new(),
            format!("BT /U1 24 Tf 50 700 Td {hex} Tj ET").into_bytes(),
        ));
        let page = doc.get_dictionary_mut(page_id).unwrap();
        page.set("Contents", Object::Reference(content));
        page.set(
            "Resources",
            Object::Dictionary(Dictionary::from_iter(vec![(
                "Font",
                Object::Dictionary(Dictionary::from_iter(vec![(
                    "U1",
                    Object::Reference(font_id),
                )])),
            )])),
        );
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        (out, font_bytes)
    }

    #[test]
    fn type0_font_round_trips_cyrillic() {
        let (bytes, _) = page_with_unicode_text("Привет Ελλάδα Łódź");
        let doc = Document::from_bytes(bytes, None).unwrap();
        let text = doc.page_text(0).unwrap();
        assert!(text.contains("Привет Ελλάδα Łódź"), "{text:?}");
    }

    #[test]
    fn type0_subset_is_small() {
        let (_, font_bytes) = page_with_unicode_text("Привет Ελλάδα Łódź");
        assert!(font_bytes < 60_000, "{font_bytes} octets");
        assert!(font_bytes > 500, "police vide ? {font_bytes}");
    }

    #[test]
    fn unmapped_char_keeps_its_unicode() {
        let (bytes, _) = page_with_unicode_text("a你");
        let doc = Document::from_bytes(bytes, None).unwrap();
        let text = doc.page_text(0).unwrap();
        assert!(text.contains("a你"), "{text:?}");
    }

    #[test]
    fn encode_assigns_cids_in_order_of_appearance() {
        let mut w = Type0Writer::new();
        assert_eq!(w.encode("aba"), "<000100020001>");
        assert_eq!(w.encode("\nc"), "<0003>");
        assert!(to_unicode_cmap(&['𝄞']).contains("<0001> <D834DD1E>"));
    }

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
