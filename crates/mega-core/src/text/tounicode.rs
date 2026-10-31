// SPDX-License-Identifier: GPL-3.0-or-later
//! Repli de la couche de texte : quand PDFium n'expose **aucun** caractère d'une page qui porte
//! pourtant un objet texte, on relit son contenu avec `lopdf` et on traduit chaque code de
//! caractère par la `/ToUnicode` de sa police (audit L2, Lot 14).
//!
//! Le cas mesuré : une police CID **CFF enveloppée en OpenType** (`/FontFile3 /Subtype
//! /OpenType`, comme Noto Sans CJK ou Source Han) dont les CID ne sont pas les numéros de
//! glyphe. PDFium prend le CID pour un numéro de glyphe, n'en trouve pas, et l'objet texte n'a
//! ni largeur ni caractère : rien à sélectionner, chercher ou copier, alors que poppler, MuPDF
//! et pdf.js lisent le texte. PDFium ne dessine pas non plus le glyphe : cela, le repli ne le
//! corrige pas (il ne touche qu'à la couche de texte).
//!
//! Ce que le repli calcule (ISO 32000-1, §9.4) : pour chaque code d'une chaîne `Tj`, `TJ`, `'`
//! ou `"`, le texte que donne la `/ToUnicode`, et une emprise : la chasse du glyphe (`/W` et
//! `/DW` d'une police composite, `/Widths` d'une police simple) sur la hauteur du cadratin
//! (`/Ascent` et `/Descent` du descripteur, bornés), placée par la matrice de rendu
//! `[Tfs × Th, 0, 0, Tfs, 0, Trise] × Tm × CTM`. Les formulaires XObject (`Do`) sont suivis.
//!
//! Limites, car c'est un repli et non un second moteur de texte :
//! - une police **sans** `/ToUnicode` ne donne aucun caractère (pas de décodage par
//!   `/Encoding`) ; ses glyphes font quand même avancer la position ;
//! - l'écriture verticale (`Identity-V`) est placée comme de l'horizontale ;
//! - les codes sont lus sur deux octets pour une police composite (un seul si sa
//!   `/ToUnicode` déclare un espace de codes d'un octet), sur un octet pour une police simple :
//!   les codages à longueur variable ne sont pas suivis ;
//! - rien n'est écarté pour invisibilité (rognage, mode de rendu 3 ou 7), comme dans PDFium ;
//! - bornes : [`MAX_CHARS`] caractères, [`MAX_FORM_DEPTH`] niveaux de formulaires,
//!   [`MAX_OPERATIONS`] opérateurs lus, [`MAX_ENTRIES`] entrées (et chaînes de destination
//!   gardées) par `/ToUnicode` ou `/W`, [`MAX_ARRAY_ITEMS`] destinations par tableau de
//!   `bfrange` (et pas plus que sa plage n'a de codes), [`MAX_FONT_LOADS`] polices,
//!   [`MAX_SAVED_STATES`] niveaux de `q`, et [`MAX_DECODED_BYTES`] décompressés en tout
//!   (`/ToUnicode` et formulaires) : au-delà, la lecture s'arrête et rend ce qu'elle a. Une
//!   `/ToUnicode` partagée par plusieurs polices n'est lue qu'une fois.
//!
//! \[Apprentissage\] **Pile explicite au lieu de récursion** : un formulaire XObject peut en
//! appeler un autre, et un fichier hostile peut en enchaîner des milliers. Plutôt qu'une
//! fonction qui s'appelle elle-même (et ferait déborder la pile du programme), on garde un
//! `Vec<Frame>` : entrer dans un formulaire = empiler un cadre, en sortir = le dépiler. La
//! profondeur se compte avec `frames.len()` (voir `docs/apprentissage/lot-13.md`).
//!
//! \[Apprentissage\] **`Rc`** (compteur de références) : plusieurs cadres partagent la même
//! liste d'opérateurs d'un formulaire appelé plusieurs fois ; `Rc::clone` copie un pointeur et
//! incrémente un compteur, jamais la liste elle-même.

use std::collections::BTreeMap;
use std::rc::Rc;

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document as LoDocument, Object, ObjectId};

use crate::content::raster::{IDENTITY, Matrix, apply, matrix_operands, multiply};
use crate::geometry::Rect;

/// Nombre maximal de caractères rendus par le repli pour une page.
pub(super) const MAX_CHARS: usize = 20_000;
/// Profondeur maximale de formulaires XObject imbriqués.
pub(super) const MAX_FORM_DEPTH: usize = 8;
/// Nombre maximal d'opérateurs lus pour une page, formulaires compris (un formulaire appelé
/// mille fois compte mille fois).
pub(super) const MAX_OPERATIONS: usize = 1_000_000;
/// Nombre maximal d'entrées lues dans une `/ToUnicode` ou un tableau `/W`, et de chaînes de
/// destination gardées par une `/ToUnicode` (`bfchar` et éléments de listes réunis).
pub(super) const MAX_ENTRIES: usize = 100_000;
/// Nombre maximal de destinations lues dans un tableau de `bfrange` (`<lo> <hi> [ … ]`).
pub(super) const MAX_ARRAY_ITEMS: usize = 4_096;
/// Nombre maximal de polices analysées pour une page (par référence ou directes) : au-delà,
/// une police inconnue ne donne aucun caractère.
pub(super) const MAX_FONT_LOADS: usize = 256;
/// Profondeur maximale de la pile `q` : un `q` plus profond est ignoré (avec son `Q`).
pub(super) const MAX_SAVED_STATES: usize = 64;
/// Octets décompressés au plus pour une page, `/ToUnicode` et formulaires XObject compris :
/// mille polices qui désignent chacune une « bombe de décompression » ne bloquent pas le
/// programme (ADR-0017).
pub(super) const MAX_DECODED_BYTES: usize = 64 * 1024 * 1024;

/// Un caractère retrouvé par le repli.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct FoundChar {
    /// Le caractère.
    pub(super) ch: char,
    /// Son emprise, en espace **utilisateur**.
    pub(super) rect: Rect,
    /// Son angle en degrés (0 à 360), mesuré comme `FPDFText_GetCharAngle` de PDFium.
    pub(super) angle: f32,
}

/// Destination d'une plage `bfrange` d'une `/ToUnicode`.
enum Target {
    /// `<lo> <hi> <dst>` : le code `lo + k` donne `dst` dont la dernière unité UTF-16 est
    /// augmentée de `k`.
    Start(Vec<u16>),
    /// `<lo> <hi> [<d0> <d1> …]` : une destination par code.
    List(Vec<String>),
}

/// Une `/ToUnicode` lue : codes de caractère → texte Unicode.
#[derive(Default)]
pub(super) struct ToUnicode {
    /// Entrées `bfchar`.
    chars: BTreeMap<u32, String>,
    /// Entrées `bfrange`, rangées par premier code : `lo → (hi, destination)`.
    ranges: BTreeMap<u32, (u32, Target)>,
    /// Longueur en octets des codes, d'après la première `codespacerange`.
    code_len: Option<usize>,
    /// Chaînes de destination gardées jusqu'ici (`bfchar` et éléments des listes de
    /// `bfrange`), plafonnées à [`MAX_ENTRIES`] : compter les seules entrées laisserait chaque
    /// liste garder ses milliers de chaînes, soit des centaines de millions pour un CMap de
    /// quelques dizaines de Mio fait de `<>`.
    stored: usize,
}

/// Un élément lexical d'un CMap (PostScript réduit à ce dont on a besoin).
enum Token {
    /// Chaîne hexadécimale `<…>`, décodée.
    Hex(Vec<u8>),
    /// Début de tableau `[`.
    ArrayStart,
    /// Fin de tableau `]`.
    ArrayEnd,
    /// Mot-clé (`beginbfchar`, `def`…).
    Keyword(Vec<u8>),
    /// Tout le reste (nombre, nom, chaîne littérale, dictionnaire) : ignoré.
    Other,
}

/// Valeur d'un chiffre hexadécimal.
fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Vrai pour un délimiteur PostScript (fin d'un mot).
fn is_delimiter(b: u8) -> bool {
    b.is_ascii_whitespace() || b == 0 || b"()<>[]{}/%".contains(&b)
}

/// Lecteur d'éléments d'un CMap, octet par octet.
struct Lexer<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Lexer<'_> {
    /// L'élément suivant, ou `None` en fin de données.
    fn next_token(&mut self) -> Option<Token> {
        loop {
            let b = *self.data.get(self.pos)?;
            if b.is_ascii_whitespace() || b == 0 {
                self.pos += 1;
            } else if b == b'%' {
                while self
                    .data
                    .get(self.pos)
                    .is_some_and(|&c| c != b'\n' && c != b'\r')
                {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
        let b = *self.data.get(self.pos)?;
        self.pos += 1;
        match b {
            b'[' => Some(Token::ArrayStart),
            b']' => Some(Token::ArrayEnd),
            b'<' if self.data.get(self.pos) == Some(&b'<') => {
                self.pos += 1;
                Some(Token::Other)
            }
            b'>' => {
                if self.data.get(self.pos) == Some(&b'>') {
                    self.pos += 1;
                }
                Some(Token::Other)
            }
            b'<' => {
                let mut digits = Vec::new();
                while let Some(&c) = self.data.get(self.pos) {
                    self.pos += 1;
                    if c == b'>' {
                        break;
                    }
                    digits.extend(hex_digit(c));
                }
                // Un nombre impair de chiffres : le dernier est complété par 0 (§7.3.4.3).
                Some(Token::Hex(
                    digits
                        .chunks(2)
                        .map(|p| (p[0] << 4) | p.get(1).copied().unwrap_or(0))
                        .collect(),
                ))
            }
            b'(' => {
                // Chaîne littérale : parenthèses équilibrées, `\` échappe le caractère suivant.
                let mut depth = 1usize;
                while let Some(&c) = self.data.get(self.pos) {
                    self.pos += 1;
                    match c {
                        b'\\' => self.pos += 1,
                        b'(' => depth += 1,
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                Some(Token::Other)
            }
            _ => {
                let start = self.pos - 1;
                while self.data.get(self.pos).is_some_and(|&c| !is_delimiter(c)) {
                    self.pos += 1;
                }
                let word = self.data.get(start..self.pos).unwrap_or_default();
                // Un nom (`/CMapName`), un nombre ou une accolade ne sont pas des mots-clés.
                if b.is_ascii_alphabetic() {
                    Some(Token::Keyword(word.to_vec()))
                } else {
                    Some(Token::Other)
                }
            }
        }
    }
}

/// Code de caractère (gros-boutiste) écrit sur 1 à 4 octets ; `None` au-delà.
fn code_of(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() || bytes.len() > 4 {
        return None;
    }
    Some(bytes.iter().fold(0u32, |acc, &b| (acc << 8) | u32::from(b)))
}

/// Unités UTF-16 gros-boutistes d'une destination (un octet final isolé est ignoré).
fn utf16_units(bytes: &[u8]) -> Vec<u16> {
    // [Apprentissage] `as_chunks::<2>()` découpe la tranche en tableaux `[u8; 2]` et rend à
    // part le reste (ici un octet isolé, ignoré) ; voir aussi `cleanup/mod.rs`.
    let (pairs, _odd) = bytes.as_chunks::<2>();
    pairs.iter().map(|&pair| u16::from_be_bytes(pair)).collect()
}

/// Texte d'une suite d'unités UTF-16 ; une moitié de paire isolée est ignorée.
fn utf16_text(units: &[u16]) -> String {
    char::decode_utf16(units.iter().copied())
        .filter_map(Result::ok)
        .collect()
}

/// Nombre de codes de la plage `lo..=hi` (`hi >= lo`), plafonné à [`MAX_ARRAY_ITEMS`].
fn range_len(lo: u32, hi: u32) -> usize {
    usize::try_from(hi - lo)
        .map_or(MAX_ARRAY_ITEMS, |n| n.saturating_add(1))
        .min(MAX_ARRAY_ITEMS)
}

/// Nombre de destinations qu'une liste `[ … ]` de `bfrange` peut garder : autant que sa plage
/// a de codes (`<lo> <hi>`, les deux opérandes qui la précèdent), au plus [`MAX_ARRAY_ITEMS`].
/// Une liste qui ne suit pas deux codes valides n'en garde aucune : elle sera écartée.
fn list_room(operands: &[Operand]) -> usize {
    let [.., Operand::Hex(lo), Operand::Hex(hi)] = operands else {
        return 0;
    };
    match (code_of(lo), code_of(hi)) {
        (Some(lo), Some(hi)) if hi >= lo => range_len(lo, hi),
        _ => 0,
    }
}

/// Un opérande d'un CMap, entre deux mots-clés.
enum Operand {
    /// Chaîne hexadécimale.
    Hex(Vec<u8>),
    /// Tableau de destinations (`bfrange` à liste), déjà décodées.
    List(Vec<String>),
    /// Autre chose (nombre, nom…).
    Other,
}

impl ToUnicode {
    /// Lit un CMap `/ToUnicode` : `codespacerange` (longueur des codes), `bfchar`, `bfrange`.
    /// Un CMap abîmé donne ce qu'on a pu en lire, jamais d'erreur.
    pub(super) fn parse(data: &[u8]) -> Self {
        let mut map = ToUnicode::default();
        let mut lexer = Lexer { data, pos: 0 };
        // Opérandes depuis le dernier mot-clé ; tableau en cours de lecture, avec le nombre
        // de destinations qu'il a le droit de garder.
        let mut operands: Vec<Operand> = Vec::new();
        let mut array: Option<(Vec<String>, usize)> = None;
        while let Some(token) = lexer.next_token() {
            if map.len() >= MAX_ENTRIES || operands.len() > 3 * MAX_ENTRIES {
                break;
            }
            match token {
                Token::ArrayStart => array = Some((Vec::new(), list_room(&operands))),
                Token::ArrayEnd => {
                    if let Some((items, _)) = array.take() {
                        operands.push(Operand::List(items));
                    }
                }
                Token::Hex(bytes) => match array.as_mut() {
                    Some((items, room)) => {
                        if items.len() < *room && map.stored < MAX_ENTRIES {
                            items.push(utf16_text(&utf16_units(&bytes)));
                            map.stored += 1;
                        }
                    }
                    None => operands.push(Operand::Hex(bytes)),
                },
                Token::Keyword(word) => {
                    match word.as_slice() {
                        b"endcodespacerange" => {
                            if map.code_len.is_none()
                                && let Some(Operand::Hex(lo)) = operands.first()
                            {
                                map.code_len = Some(lo.len());
                            }
                        }
                        b"endbfchar" => map.add_chars(&operands),
                        b"endbfrange" => map.add_ranges(std::mem::take(&mut operands)),
                        _ => {}
                    }
                    operands.clear();
                    array = None;
                }
                Token::Other => {
                    if array.is_none() {
                        operands.push(Operand::Other);
                    }
                }
            }
        }
        map
    }

    /// Nombre d'entrées lues.
    fn len(&self) -> usize {
        self.chars.len() + self.ranges.len()
    }

    /// Nombre de chaînes de destination réellement gardées (`bfchar` et éléments de listes).
    #[cfg(test)]
    fn stored_strings(&self) -> usize {
        let lists: usize = self
            .ranges
            .values()
            .map(|(_, target)| match target {
                Target::List(items) => items.len(),
                Target::Start(_) => 0,
            })
            .sum();
        self.chars.len() + lists
    }

    /// Entrées d'une section `bfchar` : paires `<code> <destination>`.
    fn add_chars(&mut self, operands: &[Operand]) {
        for pair in operands.as_chunks::<2>().0 {
            if self.stored >= MAX_ENTRIES {
                return;
            }
            if let [Operand::Hex(src), Operand::Hex(dst)] = pair
                && let Some(code) = code_of(src)
            {
                self.chars.insert(code, utf16_text(&utf16_units(dst)));
                self.stored += 1;
            }
        }
    }

    /// Entrées d'une section `bfrange` : triplets `<lo> <hi> <dst>` ou `<lo> <hi> [ … ]`.
    fn add_ranges(&mut self, operands: Vec<Operand>) {
        let mut operands = operands.into_iter();
        while let (Some(lo), Some(hi), Some(dst)) =
            (operands.next(), operands.next(), operands.next())
        {
            let (Operand::Hex(lo), Operand::Hex(hi)) = (lo, hi) else {
                continue;
            };
            let (Some(lo), Some(hi)) = (code_of(&lo), code_of(&hi)) else {
                continue;
            };
            if hi < lo {
                continue;
            }
            let target = match dst {
                Operand::Hex(bytes) => Target::Start(utf16_units(&bytes)),
                Operand::List(mut items) => {
                    // Déjà borné à la lecture si `lo` et `hi` précédaient la liste ; ceinture
                    // pour un CMap dont les opérandes sont décalés.
                    items.truncate(range_len(lo, hi));
                    Target::List(items)
                }
                Operand::Other => continue,
            };
            self.ranges.insert(lo, (hi, target));
        }
    }

    /// Le texte du code `code`, s'il est défini et non vide.
    pub(super) fn lookup(&self, code: u32) -> Option<String> {
        if let Some(text) = self.chars.get(&code) {
            return (!text.is_empty()).then(|| text.clone());
        }
        let (&lo, (hi, target)) = self.ranges.range(..=code).next_back()?;
        if code > *hi {
            return None;
        }
        let offset = code - lo;
        let text = match target {
            Target::Start(units) => {
                let mut units = units.clone();
                let last = units.last_mut()?;
                *last = last.checked_add(u16::try_from(offset).ok()?)?;
                utf16_text(&units)
            }
            Target::List(items) => items.get(usize::try_from(offset).ok()?)?.clone(),
        };
        (!text.is_empty()).then_some(text)
    }
}

/// Chasses d'une police, en unités de glyphe.
struct Widths {
    /// Chasses données une à une (`c [w1 w2 …]` dans `/W`, ou `/Widths`).
    explicit: BTreeMap<u32, f32>,
    /// Plages `c_first c_last w` de `/W`, rangées par premier code.
    ranges: BTreeMap<u32, (u32, f32)>,
    /// Chasse par défaut (`/DW`, `/MissingWidth`).
    default: f32,
}

impl Widths {
    /// La chasse du code `code`.
    fn get(&self, code: u32) -> f32 {
        if let Some(&w) = self.explicit.get(&code) {
            return w;
        }
        match self.ranges.range(..=code).next_back() {
            Some((_, &(last, w))) if code <= last => w,
            _ => self.default,
        }
    }
}

/// Ce que le repli sait d'une police de la page.
struct Font {
    /// Sa `/ToUnicode`, s'il y en a une (partagée avec les polices qui citent le même flux).
    to_unicode: Option<Rc<ToUnicode>>,
    /// Codes sur deux octets (police composite) plutôt qu'un.
    two_bytes: bool,
    /// Ses chasses.
    widths: Widths,
    /// Unités de glyphe → unités de texte (1/1000, ou `/FontMatrix` d'une police Type 3).
    scale: f32,
    /// Haut et bas du cadratin, en unités de texte (fractions de la taille de police).
    ascent: f32,
    descent: f32,
}

/// L'objet `obj`, référence suivie.
fn resolve<'a>(doc: &'a LoDocument, obj: &'a Object) -> Option<&'a Object> {
    doc.dereference(obj).ok().map(|(_, o)| o)
}

/// La valeur de `key` dans `dict`, référence suivie.
fn entry<'a>(doc: &'a LoDocument, dict: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    resolve(doc, dict.get(key).ok()?)
}

/// Un nombre fini, référence suivie.
fn number(doc: &LoDocument, obj: &Object) -> Option<f32> {
    resolve(doc, obj)?.as_float().ok().filter(|v| v.is_finite())
}

/// Les chasses d'un tableau `/W` de police CID (§9.7.4.3) : `c [w1 w2 …]` ou `c_first c_last w`.
fn cid_widths(doc: &LoDocument, w: Option<&Object>, default: f32) -> Widths {
    let mut widths = Widths {
        explicit: BTreeMap::new(),
        ranges: BTreeMap::new(),
        default,
    };
    let Some(items) = w.and_then(|w| resolve(doc, w)?.as_array().ok()) else {
        return widths;
    };
    let mut i = 0;
    while i < items.len() && widths.explicit.len() + widths.ranges.len() < MAX_ENTRIES {
        let Some(first) = items.get(i).and_then(|o| number(doc, o)) else {
            break;
        };
        // Un code est un entier positif : on le tronque, comme les autres lecteurs.
        // [Apprentissage] `as` d'un `f32` vers un entier **sature** : une valeur trop grande
        // donne `u32::MAX`, une négative 0, et NaN donne 0 (jamais de comportement indéfini,
        // contrairement au C). `number` écarte déjà NaN et l'infini ; `max(0.0)` dit l'intention.
        let first = first.max(0.0) as u32;
        let next = items.get(i + 1).and_then(|o| resolve(doc, o));
        if let Some(Object::Array(list)) = next {
            for (k, value) in list.iter().take(MAX_ENTRIES).enumerate() {
                let (Some(w), Ok(k)) = (number(doc, value), u32::try_from(k)) else {
                    continue;
                };
                widths.explicit.insert(first.saturating_add(k), w);
            }
            i += 2;
        } else {
            let last = next.and_then(|o| number(doc, o));
            let w = items.get(i + 2).and_then(|o| number(doc, o));
            let (Some(last), Some(w)) = (last, w) else {
                break;
            };
            // Même conversion saturante que pour `first`.
            widths.ranges.insert(first, (last.max(0.0) as u32, w));
            i += 3;
        }
    }
    widths
}

/// Les chasses d'une police simple : `/FirstChar` et `/Widths` ; hors de la plage,
/// `/MissingWidth` du descripteur. Sans `/Widths` (police standard), un demi-cadratin.
fn simple_widths(doc: &LoDocument, font: &Dictionary, descriptor: Option<&Dictionary>) -> Widths {
    let list = entry(doc, font, b"Widths").and_then(|o| o.as_array().ok());
    let missing = descriptor
        .and_then(|d| entry(doc, d, b"MissingWidth"))
        .and_then(|o| number(doc, o));
    let mut widths = Widths {
        explicit: BTreeMap::new(),
        ranges: BTreeMap::new(),
        default: missing.unwrap_or(if list.is_some() { 0.0 } else { 500.0 }),
    };
    let first = entry(doc, font, b"FirstChar")
        .and_then(|o| number(doc, o))
        .map_or(0, |f| f.max(0.0) as u32);
    for (k, value) in list.into_iter().flatten().take(256).enumerate() {
        if let (Some(w), Ok(k)) = (number(doc, value), u32::try_from(k)) {
            widths.explicit.insert(first.saturating_add(k), w);
        }
    }
    widths
}

/// Ce que le repli retient de la police `font` (dictionnaire `/Font`), sa `/ToUnicode` déjà
/// lue (voir `Walker::cmap_of`).
fn load_font(doc: &LoDocument, font: &Dictionary, to_unicode: Option<Rc<ToUnicode>>) -> Font {
    let subtype = entry(doc, font, b"Subtype").and_then(|o| o.as_name().ok());
    let composite = subtype == Some(b"Type0".as_slice());
    let descendant = composite
        .then(|| entry(doc, font, b"DescendantFonts"))
        .flatten()
        .and_then(|o| o.as_array().ok())
        .and_then(|a| a.first())
        .and_then(|o| resolve(doc, o))
        .and_then(|o| o.as_dict().ok());
    // Le descripteur d'une police composite est dans sa police descendante.
    let descriptor = descendant
        .or((!composite).then_some(font))
        .and_then(|d| entry(doc, d, b"FontDescriptor"))
        .and_then(|o| o.as_dict().ok());
    let widths = match descendant {
        Some(cid_font) => {
            let default = entry(doc, cid_font, b"DW")
                .and_then(|o| number(doc, o))
                .unwrap_or(1000.0);
            cid_widths(doc, cid_font.get(b"W").ok(), default)
        }
        None if composite => cid_widths(doc, None, 1000.0),
        None => simple_widths(doc, font, descriptor),
    };
    // Une police Type 3 a son propre espace de glyphe (`/FontMatrix`) ; les autres, 1/1000.
    let scale = if subtype == Some(b"Type3".as_slice()) {
        entry(doc, font, b"FontMatrix")
            .and_then(|o| o.as_array().ok())
            .and_then(|m| m.first())
            .and_then(|o| number(doc, o))
            .filter(|s| *s > 0.0)
            .unwrap_or(0.001)
    } else {
        0.001
    };
    // Cadratin : `/Ascent` et `/Descent` bornés (certains fichiers y mettent la boîte de toute
    // la police, deux fois plus haute que la ligne).
    let metric = |key: &[u8]| {
        descriptor
            .and_then(|d| entry(doc, d, key))
            .and_then(|o| number(doc, o))
            .map(|v| v / 1000.0)
    };
    let ascent = metric(b"Ascent").map_or(0.8, |a| a.clamp(0.5, 1.0));
    let descent = metric(b"Descent").map_or(-0.2, |d| d.clamp(-0.3, 0.0));
    // `Identity-H` et `Identity-V` lisent toujours deux octets, quoi que dise la `/ToUnicode` ;
    // pour un autre CMap, on suit l'espace de codes qu'elle déclare.
    let identity = matches!(
        entry(doc, font, b"Encoding").and_then(|o| o.as_name().ok()),
        Some(b"Identity-H" | b"Identity-V")
    );
    let two_bytes = composite
        && (identity
            || to_unicode
                .as_ref()
                .is_none_or(|map| map.code_len != Some(1)));
    Font {
        to_unicode,
        two_bytes,
        widths,
        scale,
        ascent,
        descent,
    }
}

/// L'état graphique qui compte pour le texte (sauvé par `q`, rendu par `Q`, §8.4 et §9.3).
#[derive(Clone)]
struct GraphicsState {
    /// Matrice courante (`cm`).
    ctm: Matrix,
    /// Police courante : index dans `Walker::fonts`.
    font: Option<usize>,
    /// Taille de police (`Tf`).
    size: f32,
    /// Espacement des caractères (`Tc`).
    char_spacing: f32,
    /// Espacement des mots (`Tw`).
    word_spacing: f32,
    /// Échelle horizontale (`Tz` / 100).
    horizontal_scale: f32,
    /// Interligne (`TL`).
    leading: f32,
    /// Montée (`Ts`).
    rise: f32,
}

/// Un flux de contenu en cours de lecture : la page, ou un formulaire XObject.
struct Frame {
    /// Ses opérateurs.
    ops: Rc<Vec<Operation>>,
    /// Le prochain à lire.
    next: usize,
    /// Ses ressources (polices, XObjects).
    resources: Rc<Dictionary>,
    /// Le formulaire lu (`None` pour la page) : un formulaire déjà sur la pile n'y entre pas
    /// une seconde fois (cycle).
    form: Option<ObjectId>,
    /// Ce qu'il faut rendre en sortant du formulaire (`None` pour la page).
    saved: Option<Saved>,
}

/// L'état de l'appelant d'un formulaire XObject, rendu à la sortie du formulaire.
struct Saved {
    /// État graphique.
    state: GraphicsState,
    /// Hauteur de la pile `q`.
    depth: usize,
    /// Nombre de `q` ignorés (pile pleine) en attente de leur `Q`.
    ignored: usize,
}

/// Un formulaire XObject décodé : ses opérateurs, et ses `/Resources` propres s'il en a.
type DecodedForm = (Rc<Vec<Operation>>, Option<Rc<Dictionary>>);

/// Le lecteur de contenu.
struct Walker<'a> {
    doc: &'a LoDocument,
    /// Polices déjà analysées.
    fonts: Vec<Font>,
    /// Police analysée par objet de police.
    font_ids: BTreeMap<ObjectId, usize>,
    /// `/ToUnicode` déjà lues, par flux (`None` : illisible).
    cmaps: BTreeMap<ObjectId, Option<Rc<ToUnicode>>>,
    /// Formulaires déjà décodés (`None` : illisibles, on n'y revient pas à chaque `Do`).
    forms: BTreeMap<ObjectId, Option<DecodedForm>>,
    /// Octets décompressés qu'il reste le droit de produire.
    budget: usize,
    /// Vrai quand le budget d'octets est épuisé : la lecture s'arrête.
    exhausted: bool,
    state: GraphicsState,
    /// Pile des états sauvés par `q`.
    saved: Vec<GraphicsState>,
    /// `q` ignorés parce que la pile était pleine : leurs `Q` sont ignorés aussi.
    ignored: usize,
    /// Matrice de texte (`Tm`) et de début de ligne (`Tlm`).
    tm: Matrix,
    tlm: Matrix,
    out: Vec<FoundChar>,
}

/// Translation `(tx, ty)`.
fn translation(tx: f32, ty: f32) -> Matrix {
    [1.0, 0.0, 0.0, 1.0, tx, ty]
}

/// Le `n`-ième opérande d'un opérateur, en nombre.
fn operand(op: &Operation, n: usize) -> Option<f32> {
    op.operands
        .get(n)
        .and_then(|o| o.as_float().ok())
        .filter(|v| v.is_finite())
}

impl<'a> Walker<'a> {
    fn new(doc: &'a LoDocument, budget: usize) -> Self {
        Walker {
            doc,
            fonts: Vec::new(),
            font_ids: BTreeMap::new(),
            cmaps: BTreeMap::new(),
            forms: BTreeMap::new(),
            budget,
            exhausted: false,
            state: GraphicsState {
                ctm: IDENTITY,
                font: None,
                size: 0.0,
                char_spacing: 0.0,
                word_spacing: 0.0,
                horizontal_scale: 1.0,
                leading: 0.0,
                rise: 0.0,
            },
            saved: Vec::new(),
            ignored: 0,
            tm: IDENTITY,
            tlm: IDENTITY,
            out: Vec::new(),
        }
    }

    /// La police nommée `name` dans `resources`, analysée une fois par objet de police.
    fn font(&mut self, resources: &Dictionary, name: &[u8]) -> Option<usize> {
        let doc = self.doc;
        let fonts = entry(doc, resources, b"Font")?.as_dict().ok()?;
        let value = fonts.get(name).ok()?;
        let (id, object) = doc.dereference(value).ok()?;
        if let Some(index) = id.and_then(|id| self.font_ids.get(&id)) {
            return Some(*index);
        }
        if self.fonts.len() >= MAX_FONT_LOADS {
            return None;
        }
        let dict = object.as_dict().ok()?;
        let to_unicode = self.cmap_of(dict);
        let font = load_font(doc, dict, to_unicode);
        self.fonts.push(font);
        let index = self.fonts.len() - 1;
        if let Some(id) = id {
            self.font_ids.insert(id, index);
        }
        Some(index)
    }

    /// Les octets décodés de `stream`, décomptés du budget ; `None` (et lecture arrêtée) si
    /// le budget ne suffit pas.
    fn decode(&mut self, stream: &lopdf::Stream) -> Option<Vec<u8>> {
        if self.exhausted {
            return None;
        }
        let Some(data) = crate::crypt::stream_bytes_within(stream, self.budget) else {
            self.exhausted = true;
            return None;
        };
        self.budget = self.budget.saturating_sub(data.len());
        Some(data)
    }

    /// La `/ToUnicode` de la police `font`, lue une seule fois par flux.
    fn cmap_of(&mut self, font: &Dictionary) -> Option<Rc<ToUnicode>> {
        let doc = self.doc;
        let (id, object) = doc.dereference(font.get(b"ToUnicode").ok()?).ok()?;
        if let Some(cached) = id.and_then(|id| self.cmaps.get(&id)) {
            return cached.clone();
        }
        let parsed = object
            .as_stream()
            .ok()
            .and_then(|stream| self.decode(stream))
            .map(|data| Rc::new(ToUnicode::parse(&data)));
        if let Some(id) = id {
            self.cmaps.insert(id, parsed.clone());
        }
        parsed
    }

    /// Passe à la ligne suivante (`T*`).
    fn next_line(&mut self) {
        self.tlm = multiply(&translation(0.0, -self.state.leading), &self.tlm);
        self.tm = self.tlm;
    }

    /// Dessine la chaîne `bytes` (`Tj`) : un caractère par code traduit, puis l'avance.
    fn show(&mut self, bytes: &[u8]) {
        let Some(font) = self.state.font.and_then(|i| self.fonts.get(i)) else {
            return;
        };
        let state = &self.state;
        let step = if font.two_bytes { 2 } else { 1 };
        for chunk in bytes.chunks_exact(step) {
            let Some(code) = code_of(chunk) else {
                continue;
            };
            let advance = font.widths.get(code) * font.scale;
            let text = font.to_unicode.as_ref().and_then(|map| map.lookup(code));
            if let Some(text) = text
                && self.out.len() < MAX_CHARS
            {
                // Matrice de rendu (§9.4.4) : espace du texte → espace utilisateur.
                let size = [
                    state.size * state.horizontal_scale,
                    0.0,
                    0.0,
                    state.size,
                    0.0,
                    state.rise,
                ];
                let trm = multiply(&multiply(&size, &self.tm), &state.ctm);
                // Même mesure que `FPDFText_GetCharAngle` de PDFium : atan2(-b, a) de la matrice
                // du caractère, en espace utilisateur, dans le sens horaire.
                let angle = (-trm[1]).atan2(trm[0]).to_degrees().rem_euclid(360.0);
                // Une ligature (« ﬁ » → « fi ») partage la chasse du glyphe entre ses lettres.
                let count = text.chars().count() as f32;
                for (k, ch) in text.chars().enumerate() {
                    let x0 = advance * k as f32 / count;
                    let x1 = advance * (k + 1) as f32 / count;
                    let corners = [
                        (x0, font.descent),
                        (x1, font.descent),
                        (x0, font.ascent),
                        (x1, font.ascent),
                    ]
                    .map(|(x, y)| apply(&trm, x, y));
                    let (mut left, mut bottom) = (f32::MAX, f32::MAX);
                    let (mut right, mut top) = (f32::MIN, f32::MIN);
                    for (x, y) in corners {
                        left = left.min(x);
                        bottom = bottom.min(y);
                        right = right.max(x);
                        top = top.max(y);
                    }
                    let rect = Rect {
                        x: left,
                        y: bottom,
                        w: right - left,
                        h: top - bottom,
                    };
                    if [rect.x, rect.y, rect.w, rect.h]
                        .iter()
                        .all(|v| v.is_finite())
                        && self.out.len() < MAX_CHARS
                    {
                        self.out.push(FoundChar { ch, rect, angle });
                    }
                }
            }
            // Avance (§9.4.4) : chasse × taille, plus `Tc`, plus `Tw` pour l'espace d'un octet.
            let word = if step == 1 && code == 32 {
                state.word_spacing
            } else {
                0.0
            };
            let tx = (advance * state.size + state.char_spacing + word) * state.horizontal_scale;
            self.tm = multiply(&translation(tx, 0.0), &self.tm);
        }
    }

    /// Les opérateurs et les `/Resources` propres du formulaire `stream`, ou `None` s'il est
    /// illisible (ou si le budget d'octets ne suffit pas).
    fn decode_form(&mut self, stream: &lopdf::Stream) -> Option<DecodedForm> {
        let data = self.decode(stream)?;
        let ops = Content::decode(&data).ok()?.operations;
        let own = entry(self.doc, &stream.dict, b"Resources")
            .and_then(|o| o.as_dict().ok())
            .map(|d| Rc::new(d.clone()));
        Some((Rc::new(ops), own))
    }

    /// Le cadre du formulaire XObject nommé `name` (opérateur `Do`), ou `None` si ce n'est pas
    /// un formulaire lisible, s'il est déjà sur la pile (cycle) ou trop profond.
    fn enter_form(&mut self, frames: &[Frame], name: &[u8]) -> Option<Frame> {
        let doc = self.doc;
        if frames.len() > MAX_FORM_DEPTH {
            return None;
        }
        let resources = &frames.last()?.resources;
        let id = entry(doc, resources, b"XObject")?
            .as_dict()
            .ok()?
            .get(name)
            .ok()?
            .as_reference()
            .ok()?;
        if frames.iter().any(|f| f.form == Some(id)) {
            return None;
        }
        let stream = doc.get_object(id).ok()?.as_stream().ok()?;
        if entry(doc, &stream.dict, b"Subtype").and_then(|o| o.as_name().ok())
            != Some(b"Form".as_slice())
        {
            return None;
        }
        // Décodé une seule fois, qu'il soit lisible ou non : un formulaire illisible appelé
        // mille fois ne se décompresse pas mille fois (ni ne se décompte mille fois du budget).
        let (ops, own) = match self.forms.get(&id) {
            Some(cached) => cached.clone()?,
            None => {
                let decoded = self.decode_form(stream);
                self.forms.insert(id, decoded.clone());
                decoded?
            }
        };
        // Sans `/Resources` propres, un formulaire utilise celles de son appelant.
        let form_resources = own.unwrap_or_else(|| Rc::clone(resources));
        let matrix = entry(doc, &stream.dict, b"Matrix")
            .and_then(|o| o.as_array().ok())
            .and_then(|m| {
                let values: Vec<f32> = m.iter().filter_map(|o| number(doc, o)).collect();
                <[f32; 6]>::try_from(values.as_slice()).ok()
            })
            .unwrap_or(IDENTITY);
        let saved = Saved {
            state: self.state.clone(),
            depth: self.saved.len(),
            ignored: self.ignored,
        };
        self.state.ctm = multiply(&matrix, &self.state.ctm);
        Some(Frame {
            ops,
            next: 0,
            resources: form_resources,
            form: Some(id),
            saved: Some(saved),
        })
    }

    /// Applique un opérateur ; rend le cadre d'un formulaire où entrer. `base` : hauteur de la
    /// pile `q` et nombre de `q` ignorés à l'entrée du flux courant.
    fn step(&mut self, frames: &[Frame], op: &Operation, base: (usize, usize)) -> Option<Frame> {
        // [Apprentissage] Emprunts **disjoints** de champs : `state` emprunte mutablement le
        // seul champ `self.state`, et le compilateur accepte qu'on touche en même temps à
        // `self.saved` ou `self.ignored` (d'autres champs). Il refuserait en revanche un appel de
        // méthode `self.font(…)` tant que `state` vit : une méthode emprunte tout `self`. D'où
        // `self.state.font = …` (et non `state.font`) dans la branche `Tf`.
        let state = &mut self.state;
        match op.operator.as_str() {
            "q" => {
                if self.saved.len() < MAX_SAVED_STATES {
                    self.saved.push(state.clone());
                } else {
                    self.ignored += 1;
                }
            }
            "Q" => {
                // Le `Q` d'un `q` ignoré est ignoré ; un `Q` en trop dans un formulaire ne
                // rend pas l'état de son appelant.
                if self.ignored > base.1 {
                    self.ignored -= 1;
                } else if self.saved.len() > base.0
                    && let Some(previous) = self.saved.pop()
                {
                    self.state = previous;
                }
            }
            "cm" => {
                if let Some(m) = matrix_operands(op) {
                    state.ctm = multiply(&m, &state.ctm);
                }
            }
            "BT" => {
                self.tm = IDENTITY;
                self.tlm = IDENTITY;
            }
            "Tf" => {
                let name = op.operands.first().and_then(|o| o.as_name().ok());
                let resources = frames.last().map(|f| Rc::clone(&f.resources));
                self.state.font = match (name, resources) {
                    (Some(name), Some(resources)) => self.font(&resources, name),
                    _ => None,
                };
                self.state.size = operand(op, 1).unwrap_or(0.0);
            }
            "Tc" => state.char_spacing = operand(op, 0).unwrap_or(0.0),
            "Tw" => state.word_spacing = operand(op, 0).unwrap_or(0.0),
            "Tz" => state.horizontal_scale = operand(op, 0).unwrap_or(100.0) / 100.0,
            "TL" => state.leading = operand(op, 0).unwrap_or(0.0),
            "Ts" => state.rise = operand(op, 0).unwrap_or(0.0),
            "Td" | "TD" => {
                let (tx, ty) = (operand(op, 0).unwrap_or(0.0), operand(op, 1).unwrap_or(0.0));
                if op.operator == "TD" {
                    state.leading = -ty;
                }
                self.tlm = multiply(&translation(tx, ty), &self.tlm);
                self.tm = self.tlm;
            }
            "Tm" => {
                if let Some(m) = matrix_operands(op) {
                    self.tm = m;
                    self.tlm = m;
                }
            }
            "T*" => self.next_line(),
            "Tj" => {
                if let Some(bytes) = op.operands.first().and_then(|o| o.as_str().ok()) {
                    self.show(bytes);
                }
            }
            "'" => {
                self.next_line();
                if let Some(bytes) = op.operands.first().and_then(|o| o.as_str().ok()) {
                    self.show(bytes);
                }
            }
            "\"" => {
                state.word_spacing = operand(op, 0).unwrap_or(0.0);
                state.char_spacing = operand(op, 1).unwrap_or(0.0);
                self.next_line();
                if let Some(bytes) = op.operands.get(2).and_then(|o| o.as_str().ok()) {
                    self.show(bytes);
                }
            }
            "TJ" => {
                let items = op.operands.first().and_then(|o| o.as_array().ok());
                for item in items.into_iter().flatten() {
                    match item {
                        Object::String(bytes, _) => self.show(bytes),
                        other => {
                            // Un nombre recule (ou avance) de millièmes de la taille de police.
                            if let Ok(adjust) = other.as_float()
                                && adjust.is_finite()
                            {
                                let tx = -adjust / 1000.0
                                    * self.state.size
                                    * self.state.horizontal_scale;
                                self.tm = multiply(&translation(tx, 0.0), &self.tm);
                            }
                        }
                    }
                }
            }
            "Do" => {
                let name = op.operands.first().and_then(|o| o.as_name().ok())?;
                return self.enter_form(frames, name);
            }
            _ => {}
        }
        None
    }
}

/// Les caractères que dessine la page `page_id`, traduits par les `/ToUnicode` de ses polices,
/// dans l'ordre de dessin. Un contenu illisible donne ce qu'on a pu en lire.
pub(super) fn page_chars(doc: &LoDocument, page_id: ObjectId) -> Vec<FoundChar> {
    walk(doc, page_id, MAX_DECODED_BYTES).out
}

/// La lecture de [`page_chars`], avec un budget de `budget` octets décompressés ; rend le
/// lecteur pour que les tests examinent ses caches.
fn walk(doc: &LoDocument, page_id: ObjectId, budget: usize) -> Walker<'_> {
    let mut walker = Walker::new(doc, budget);
    let Ok(page) = doc.get_dictionary(page_id) else {
        return walker;
    };
    let resources = Rc::new(crate::overlay::inherited_resources(doc, page));
    let Some(content) = crate::crypt::page_content(doc, page_id)
        .ok()
        .and_then(|data| Content::decode(&data).ok())
    else {
        return walker;
    };
    let mut frames = vec![Frame {
        ops: Rc::new(content.operations),
        next: 0,
        resources,
        form: None,
        saved: None,
    }];
    let mut budget = MAX_OPERATIONS;
    while budget > 0 && walker.out.len() < MAX_CHARS && !walker.exhausted {
        let Some(frame) = frames.last_mut() else {
            break;
        };
        // [Apprentissage] `frame` emprunte mutablement `frames` ; `walker.step(&frames, …)`
        // a besoin de l'emprunter à son tour. `Rc::clone` donne une poignée **à nous** sur la
        // liste d'opérateurs (un pointeur et un compteur, pas une copie) : `op` en dépend au
        // lieu de dépendre de `frames`, et l'emprunt de `frame` peut finir avant `step`.
        let ops = Rc::clone(&frame.ops);
        let Some(op) = ops.get(frame.next) else {
            // Fin du flux : on sort du formulaire et l'appelant retrouve son état.
            if let Some(Frame {
                saved: Some(saved), ..
            }) = frames.pop()
            {
                walker.state = saved.state;
                walker.saved.truncate(saved.depth);
                walker.ignored = saved.ignored;
            }
            continue;
        };
        frame.next += 1;
        budget -= 1;
        let base = frame
            .saved
            .as_ref()
            .map_or((0, 0), |saved| (saved.depth, saved.ignored));
        if let Some(form) = walker.step(&frames, op, base) {
            frames.push(form);
        }
    }
    walker
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    #[test]
    fn bfchar_and_bfrange_are_read() {
        let cmap = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
            /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
            1 begincodespacerange <0000> <FFFF> endcodespacerange\n\
            2 beginbfchar <0003> <0020> <A911> <9664> endbfchar\n\
            2 beginbfrange <0010> <0012> <0041> <0020> <0021> [<00660069> <D835DF0B>] endbfrange\n\
            endcmap CMapName currentdict /CMap defineresource pop end end";
        let map = ToUnicode::parse(cmap);
        assert_eq!(map.code_len, Some(2));
        assert_eq!(map.lookup(0xA911).as_deref(), Some("除"));
        assert_eq!(map.lookup(0x0003).as_deref(), Some(" "));
        assert_eq!(map.lookup(0x0010).as_deref(), Some("A"));
        assert_eq!(map.lookup(0x0012).as_deref(), Some("C"));
        assert_eq!(map.lookup(0x0013), None);
        assert_eq!(map.lookup(0x0020).as_deref(), Some("fi"));
        assert_eq!(map.lookup(0x0021).as_deref(), Some("\u{1D70B}"));
        assert_eq!(map.lookup(0x0004), None);
    }

    #[test]
    fn one_byte_codespace_and_comments() {
        let cmap = b"% commentaire <41> <42>\n1 begincodespacerange <00> <FF> endcodespacerange\n\
            1 beginbfchar <41> <0042> endbfchar";
        let map = ToUnicode::parse(cmap);
        assert_eq!(map.code_len, Some(1));
        assert_eq!(map.lookup(0x41).as_deref(), Some("B"));
    }

    #[test]
    fn a_damaged_cmap_gives_what_it_can() {
        for cmap in [
            &b"beginbfchar <41"[..],
            b"beginbfrange <0000> <FFFF> endbfrange",
            b"beginbfchar <0041> /space <0042> <0043> endbfchar",
            b"(unbalanced ( string beginbfchar <41> <0041> endbfchar",
            b"beginbfrange <0010> <0001> <0041> endbfrange",
            b"beginbfrange <00FF> <0100> <FFFF> endbfrange",
        ] {
            let map = ToUnicode::parse(cmap);
            for code in [0, 0x41, 0xFF, 0x100, 0xFFFF] {
                let _ = map.lookup(code);
            }
        }
        // Une plage dont la destination déborderait d'une unité UTF-16 s'arrête là.
        let map = ToUnicode::parse(b"beginbfrange <00FF> <0100> <FFFF> endbfrange");
        assert_eq!(map.lookup(0x0100), None);
    }

    /// Une page qui dessine « B » elle-même et « A » à travers un formulaire XObject agrandi
    /// deux fois, qui s'appelle lui-même (cycle) : police simple, `/ToUnicode` d'un octet
    /// (code 1 → « A », 2 → « B », 3 → « C »), chasse 600.
    fn walked_page(page_content: &str, form_content: &str) -> Vec<FoundChar> {
        use lopdf::Stream;
        let mut doc = LoDocument::with_version("1.7");
        let cmap = b"1 begincodespacerange <00> <FF> endcodespacerange\n\
            1 beginbfrange <01> <03> <0041> endbfrange";
        let to_unicode = doc.add_object(Stream::new(dictionary! {}, cmap.to_vec()));
        let font = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "FirstChar" => 1,
            "Widths" => vec![600.into(), 600.into(), 600.into()],
            "ToUnicode" => to_unicode,
        });
        let form_id = doc.new_object_id();
        let resources = dictionary! {
            "Font" => dictionary! { "F1" => font },
            "XObject" => dictionary! { "X1" => form_id },
        };
        doc.objects.insert(
            form_id,
            Object::Stream(Stream::new(
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Form",
                    "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
                    "Matrix" => vec![2.into(), 0.into(), 0.into(), 2.into(), 0.into(), 0.into()],
                },
                form_content.as_bytes().to_vec(),
            )),
        );
        let content = doc.add_object(Stream::new(
            dictionary! {},
            page_content.as_bytes().to_vec(),
        ));
        let page = doc.add_object(dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
            "Contents" => content,
            "Resources" => resources,
        });
        page_chars(&doc, page)
    }

    #[test]
    fn forms_matrices_and_spacing_are_followed() {
        let chars = walked_page(
            "q 1 0 0 1 10 0 cm /X1 Do Q BT /F1 10 Tf 0 50 Td 2 Tc [<02> -1000 <03>] TJ ET",
            "BT /F1 10 Tf 5 5 Td <01> Tj ET /X1 Do",
        );
        let found: Vec<(char, f32, f32, f32)> = chars
            .iter()
            .map(|c| (c.ch, c.rect.x, c.rect.y, c.rect.w))
            .collect();
        // « A » : (5, 5) dans le formulaire, ×2 puis +10 en x ; chasse 0,6 × 10 × 2 = 12.
        // Le formulaire qui s'appelle lui-même n'est lu qu'une fois.
        // « B » : après `Q`, plus de décalage ; « C » : chasse 6 + Tc 2 + 10 (TJ -1000).
        assert_eq!(found.len(), 3, "{found:?}");
        let close = |a: f32, b: f32| (a - b).abs() < 0.01;
        let (a, b, c) = (found[0], found[1], found[2]);
        assert!(a.0 == 'A' && close(a.1, 20.0) && close(a.3, 12.0), "{a:?}");
        assert!(close(a.2, 10.0 - 0.2 * 20.0), "{a:?}");
        assert!(b.0 == 'B' && close(b.1, 0.0) && close(b.3, 6.0), "{b:?}");
        assert!(c.0 == 'C' && close(c.1, 18.0), "{c:?}");
        assert!(chars.iter().all(|c| c.angle == 0.0));
    }

    #[test]
    fn a_page_without_text_or_with_garbage_gives_nothing() {
        assert!(walked_page("0 0 10 10 re f", "").is_empty());
        assert!(walked_page("BT /F9 10 Tf <01> Tj ET", "").is_empty());
        assert!(walked_page("BT <01> Tj ET Q Q Q /Nope Do", "").is_empty());
    }

    /// La `/ToUnicode` « A, B, C » des codes 1 à 3, sur un octet.
    fn abc_cmap(doc: &mut LoDocument) -> ObjectId {
        let cmap = b"1 begincodespacerange <00> <FF> endcodespacerange\n\
            1 beginbfrange <01> <03> <0041> endbfrange";
        doc.add_object(lopdf::Stream::new(dictionary! {}, cmap.to_vec()))
    }

    /// Une police simple de chasse 600 (codes 1 à 3) qui cite la `/ToUnicode` `cmap`.
    fn simple_font(doc: &mut LoDocument, cmap: ObjectId) -> ObjectId {
        doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "FirstChar" => 1,
            "Widths" => vec![600.into(), 600.into(), 600.into()],
            "ToUnicode" => cmap,
        })
    }

    /// Une page de 200 × 200 qui dessine `content` avec les polices `fonts`.
    fn page_with(doc: &mut LoDocument, fonts: Dictionary, content: &str) -> ObjectId {
        use lopdf::Stream;
        let content = doc.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
        doc.add_object(dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
            "Contents" => content,
            "Resources" => dictionary! { "Font" => fonts },
        })
    }

    /// Une page qui dessine `content` avec la police simple « A, B, C » nommée `F1`.
    fn abc_page(content: &str) -> (LoDocument, ObjectId) {
        let mut doc = LoDocument::with_version("1.7");
        let cmap = abc_cmap(&mut doc);
        let font = simple_font(&mut doc, cmap);
        let page = page_with(&mut doc, dictionary! { "F1" => font }, content);
        (doc, page)
    }

    /// Les caractères trouvés, avec l'origine de leur boîte.
    fn found(chars: &[FoundChar]) -> Vec<(char, f32, f32)> {
        chars.iter().map(|c| (c.ch, c.rect.x, c.rect.y)).collect()
    }

    #[test]
    fn a_shared_to_unicode_is_parsed_once() {
        let mut doc = LoDocument::with_version("1.7");
        let cmap = abc_cmap(&mut doc);
        let (f1, f2) = (simple_font(&mut doc, cmap), simple_font(&mut doc, cmap));
        let page = page_with(
            &mut doc,
            dictionary! { "F1" => f1, "F2" => f2 },
            "BT /F1 10 Tf <01> Tj /F2 10 Tf <02> Tj ET",
        );
        let walker = walk(&doc, page, MAX_DECODED_BYTES);
        assert_eq!(walker.fonts.len(), 2);
        assert_eq!(walker.cmaps.len(), 1);
        let shared: Vec<&Rc<ToUnicode>> = walker
            .fonts
            .iter()
            .filter_map(|f| f.to_unicode.as_ref())
            .collect();
        assert!(shared.len() == 2 && Rc::ptr_eq(shared[0], shared[1]));
        let text: String = walker.out.iter().map(|c| c.ch).collect();
        assert_eq!(text, "AB");
    }

    #[test]
    fn the_number_of_fonts_is_capped() {
        let mut doc = LoDocument::with_version("1.7");
        let cmap = abc_cmap(&mut doc);
        let mut fonts = Dictionary::new();
        let mut content = String::from("BT ");
        for i in 0..MAX_FONT_LOADS + 50 {
            fonts.set(format!("F{i}"), simple_font(&mut doc, cmap));
            content.push_str(&format!("/F{i} 10 Tf <01> Tj "));
        }
        content.push_str("ET");
        let page = page_with(&mut doc, fonts, &content);
        let walker = walk(&doc, page, MAX_DECODED_BYTES);
        assert_eq!(walker.fonts.len(), MAX_FONT_LOADS);
        assert_eq!(walker.out.len(), MAX_FONT_LOADS);
    }

    #[test]
    fn the_byte_budget_stops_the_walk() {
        let (doc, page) = abc_page("BT /F1 10 Tf <01> Tj ET");
        let walker = walk(&doc, page, 10);
        assert!(walker.exhausted && walker.out.is_empty());
        let walker = walk(&doc, page, 1_000);
        assert!(!walker.exhausted && walker.out.len() == 1);
        assert!(walker.budget < 1_000);
    }

    /// Relecture de la partie B : un formulaire illisible appelé plusieurs fois n'est décodé
    /// (et décompté du budget) qu'une fois ; un formulaire lisible garde ses opérateurs et ses
    /// ressources d'un appel à l'autre.
    #[test]
    fn a_form_is_decoded_once_even_when_it_fails() {
        use lopdf::Stream;
        let broken: &[u8] = b"BT <01> Tj ET BI ]";
        assert!(
            Content::decode(broken).is_err(),
            "le contenu doit être illisible"
        );
        let mut doc = LoDocument::with_version("1.7");
        let cmap = abc_cmap(&mut doc);
        let font = simple_font(&mut doc, cmap);
        let form = |content: &[u8], resources: Option<Dictionary>| {
            let mut dict = dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
            };
            if let Some(resources) = resources {
                dict.set("Resources", resources);
            }
            Stream::new(dict, content.to_vec())
        };
        let bad = doc.add_object(form(broken, None));
        let good = doc.add_object(form(
            b"BT /F1 10 Tf <01> Tj ET",
            Some(dictionary! { "Font" => dictionary! { "F1" => font } }),
        ));
        let content = doc.add_object(Stream::new(
            dictionary! {},
            b"/B Do /B Do /B Do /G Do /G Do".to_vec(),
        ));
        let page = doc.add_object(dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
            "Contents" => content,
            "Resources" => dictionary! { "XObject" => dictionary! { "B" => bad, "G" => good } },
        });
        let walker = walk(&doc, page, MAX_DECODED_BYTES);
        let text: String = walker.out.iter().map(|c| c.ch).collect();
        assert_eq!(text, "AA");
        let cmap_len = doc
            .get_object(cmap)
            .unwrap()
            .as_stream()
            .unwrap()
            .content
            .len();
        let used = MAX_DECODED_BYTES - walker.budget;
        assert_eq!(
            used,
            broken.len() + b"BT /F1 10 Tf <01> Tj ET".len() + cmap_len
        );
        assert!(matches!(walker.forms.get(&bad), Some(None)));
        assert!(matches!(walker.forms.get(&good), Some(Some(_))));
    }

    /// Aller-retour : la `/ToUnicode` qu'écrit notre propre police Unicode
    /// (`fonts::unicode::Type0Writer`) se relit par le repli, paire de substitution comprise.
    #[test]
    fn our_type0_to_unicode_round_trips() {
        let text = "Привет π 𝜋 é";
        let mut writer = crate::fonts::unicode::Type0Writer::new();
        let hex = writer.encode(text);
        let mut doc = LoDocument::with_version("1.7");
        let font = writer.write(&mut doc).unwrap();
        let cmap_id = doc
            .get_dictionary(font)
            .unwrap()
            .get(b"ToUnicode")
            .unwrap()
            .as_reference()
            .unwrap();
        let stream = doc.get_object(cmap_id).unwrap().as_stream().unwrap();
        let map = ToUnicode::parse(&crate::crypt::stream_bytes(stream).unwrap());
        let distinct: Vec<char> = {
            let mut seen = Vec::new();
            for c in text.chars() {
                if !seen.contains(&c) {
                    seen.push(c);
                }
            }
            seen
        };
        for (cid, c) in (1u32..).zip(&distinct) {
            assert_eq!(map.lookup(cid), Some(c.to_string()), "CID {cid}");
        }
        let page = page_with(
            &mut doc,
            dictionary! { "F1" => font },
            &format!("BT /F1 10 Tf {hex} Tj ET"),
        );
        let read: String = page_chars(&doc, page).iter().map(|c| c.ch).collect();
        assert_eq!(read, text);
    }

    #[test]
    fn identity_h_reads_two_bytes_whatever_the_codespace() {
        use lopdf::Stream;
        let mut doc = LoDocument::with_version("1.7");
        let cmap = b"1 begincodespacerange <00> <FF> endcodespacerange\n\
            1 beginbfchar <0102> <9664> endbfchar";
        let cmap = doc.add_object(Stream::new(dictionary! {}, cmap.to_vec()));
        let font = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type0",
            "BaseFont" => "X",
            "Encoding" => "Identity-H",
            "DescendantFonts" => vec![Object::Dictionary(dictionary! {
                "Type" => "Font",
                "Subtype" => "CIDFontType0",
                "BaseFont" => "X",
            })],
            "ToUnicode" => cmap,
        });
        let page = page_with(
            &mut doc,
            dictionary! { "F1" => font },
            "BT /F1 10 Tf <0102> Tj ET",
        );
        let chars = page_chars(&doc, page);
        assert_eq!(chars.len(), 1);
        assert_eq!(chars[0].ch, '除');
        assert!((chars[0].rect.w - 10.0).abs() < 0.01, "{:?}", chars[0].rect);
    }

    #[test]
    fn line_operators_move_to_the_next_line() {
        // `TL` 12 : `T*` descend de 12 ; `TD` descend de 20 et fixe `TL` à 20 ; l'apostrophe
        // et le guillemet passent à la ligne avant d'écrire (le guillemet règle aussi `Tw`, `Tc`).
        let content = "BT /F1 10 Tf 0 100 Td 12 TL <01> Tj T* <02> Tj 0 -20 TD <03> Tj \
                       <01> ' 3 4 <02> \" ET";
        let (doc, page) = abc_page(content);
        let chars = found(&page_chars(&doc, page));
        // Boîte : du bas du cadratin (origine − 0,2 × 10) ; x à l'origine de la ligne.
        let expected = [
            ('A', 100.0),
            ('B', 88.0),
            ('C', 68.0),
            ('A', 48.0),
            ('B', 28.0),
        ];
        assert_eq!(chars.len(), expected.len(), "{chars:?}");
        for ((ch, x, y), (want, baseline)) in chars.iter().zip(expected) {
            assert_eq!(*ch, want);
            assert!(
                x.abs() < 0.01 && (y - (baseline - 2.0)).abs() < 0.01,
                "{chars:?}"
            );
        }
    }

    #[test]
    fn a_too_deep_q_is_ignored_with_its_q() {
        // Pile pleine (64 `q`), une translation, puis six `q` ignorés et leurs six `Q` : la
        // translation tient encore pour « A » ; les 64 `Q` suivants l'annulent pour « B ».
        let content = format!(
            "{} 1 0 0 1 10 0 cm {} {} BT /F1 10 Tf <01> Tj ET {} BT /F1 10 Tf <02> Tj ET",
            "q ".repeat(MAX_SAVED_STATES),
            "q ".repeat(6),
            "Q ".repeat(6),
            "Q ".repeat(MAX_SAVED_STATES),
        );
        let (doc, page) = abc_page(&content);
        let walker = walk(&doc, page, MAX_DECODED_BYTES);
        let chars = found(&walker.out);
        assert_eq!(chars.len(), 2, "{chars:?}");
        assert!(
            (chars[0].1 - 10.0).abs() < 0.01 && chars[1].1.abs() < 0.01,
            "{chars:?}"
        );
        assert!(walker.saved.is_empty() && walker.ignored == 0);
    }

    #[test]
    fn bfrange_arrays_are_capped() {
        let mut cmap = b"1 beginbfrange <0000> <FFFF> [".to_vec();
        for _ in 0..MAX_ARRAY_ITEMS + 10 {
            cmap.extend_from_slice(b"<0041> ");
        }
        cmap.extend_from_slice(b"] endbfrange");
        let map = ToUnicode::parse(&cmap);
        let last = u32::try_from(MAX_ARRAY_ITEMS).unwrap();
        assert_eq!(map.lookup(last - 1).as_deref(), Some("A"));
        assert_eq!(map.lookup(last), None);
    }

    /// Relecture de la partie B : des milliers de listes `bfrange` pleines de `<>` ne gardent
    /// pas des millions de chaînes. Le total des destinations retenues est plafonné à
    /// `MAX_ENTRIES`, et une liste n'en garde pas plus que sa plage n'a de codes.
    #[test]
    fn stored_destinations_are_capped_across_lists() {
        let mut cmap = b"1 beginbfrange ".to_vec();
        for k in 0..200u32 {
            let lo = k * 0x100;
            cmap.extend_from_slice(format!("<{lo:04X}> <{:04X}> [", lo + 0xFF).as_bytes());
            cmap.extend_from_slice(&b"<> ".repeat(MAX_ARRAY_ITEMS));
            cmap.extend_from_slice(b"] ");
        }
        cmap.extend_from_slice(b"endbfrange");
        let map = ToUnicode::parse(&cmap);
        let stored = map.stored_strings();
        assert!(stored <= MAX_ENTRIES, "{stored} chaînes gardées");
        // Une plage de 256 codes ne garde que 256 destinations, pas 4 096.
        assert_eq!(stored, 200 * 256);
        // Une plage de deux codes et une liste de trois : la troisième est ignorée.
        let map =
            ToUnicode::parse(b"1 beginbfrange <0001> <0002> [<0041> <0042> <0043>] endbfrange");
        assert_eq!(map.stored_strings(), 2);
        assert_eq!(map.lookup(2).as_deref(), Some("B"));
        // `bfchar` compte aussi : passé le plafond, rien n'est plus gardé.
        let mut cmap = format!("{} beginbfchar ", MAX_ENTRIES + 10).into_bytes();
        for code in 0..MAX_ENTRIES + 10 {
            cmap.extend_from_slice(format!("<{code:06X}> <0041> ").as_bytes());
        }
        cmap.extend_from_slice(b"endbfchar");
        assert!(ToUnicode::parse(&cmap).stored_strings() <= MAX_ENTRIES);
    }

    #[test]
    fn cid_widths_read_both_forms() {
        let doc = LoDocument::with_version("1.7");
        let w = Object::Array(vec![
            1.into(),
            Object::Array(vec![500.into(), 600.into()]),
            10.into(),
            20.into(),
            700.into(),
        ]);
        let widths = cid_widths(&doc, Some(&w), 1000.0);
        assert_eq!(widths.get(1), 500.0);
        assert_eq!(widths.get(2), 600.0);
        assert_eq!(widths.get(15), 700.0);
        assert_eq!(widths.get(21), 1000.0);
        assert_eq!(widths.get(3), 1000.0);
    }
}
