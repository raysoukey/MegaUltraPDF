// SPDX-License-Identifier: GPL-3.0-or-later
//! Couche de texte d'une page (caractères positionnés) et recherche.
//!
//! Les caractères viennent de PDFium ; quand PDFium n'en expose **aucun** sur une page qui
//! porte un objet texte, le sous-module `tounicode` les retrouve par les `/ToUnicode` des
//! polices (audit L2 : police CID CFF en OpenType dont PDFium ne trouve pas les glyphes).

use pdfium_render::prelude::{PdfPage, PdfPageObject, PdfPageObjectsCommon};
use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::engine;
use crate::error::CoreResult;
use crate::geometry::Rect;

mod tounicode;

/// Un caractère de la page avec son rectangle (points PDF, origine bas-gauche).
///
/// L'ordre des `TextBox` est l'ordre de lecture **calculé** par `reading_order::layout` (lignes,
/// colonnes, paragraphes), et non l'ordre de dessin du fichier que donne PDFium : le frontend
/// s'en sert pour la sélection (plage d'index) et la copie (correctif 0.12.2, ADR-0016).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextBox {
    /// Le caractère.
    pub ch: char,
    /// Son emprise approximative (`loose_bounds` : inclut la hauteur de ligne).
    pub rect: Rect,
    /// Numéro de sa ligne dans l'ordre de lecture (0 pour la première) : deux caractères de
    /// même numéro sont sur la même ligne, l'interface n'a plus à le deviner (un exposant).
    pub line: u32,
    /// Numéro de son bloc (paragraphe, colonne, bandeau…) dans l'ordre de lecture : la copie
    /// met une ligne vide entre deux blocs, et la souris vise d'abord le bloc le plus proche.
    pub block: u32,
}

/// Un caractère tel que le fichier le dessine : avec son orientation à l'écran.
pub(crate) struct DrawnChar {
    /// Le caractère.
    pub(crate) ch: char,
    /// Son emprise, en espace affiché.
    pub(crate) rect: Rect,
    /// Vrai s'il s'écrit horizontalement à l'écran (à 10° près), une fois `/Rotate` appliqué.
    horizontal: bool,
    /// Son angle à l'écran, en degrés (0 à 360).
    angle: f32,
}

/// Écart d'angle (en degrés) toléré pour un texte « horizontal » ou pour une même suite tournée.
const ANGLE_TOLERANCE: f32 = 10.0;

/// Options de recherche.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchOptions {
    /// Respecter la casse.
    pub match_case: bool,
    /// Mots entiers seulement.
    pub whole_word: bool,
}

/// Une occurrence trouvée : la page et les rectangles à surligner (un par segment de ligne).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    /// Index de page (à partir de 0).
    pub page: u16,
    /// Rectangles de l'occurrence, en points PDF.
    pub rects: Vec<Rect>,
}

impl Document {
    /// Tous les caractères de la page `index`, dans l'ordre de lecture, avec leurs numéros de
    /// ligne et de bloc.
    pub fn text_boxes(&self, index: u16) -> CoreResult<Vec<TextBox>> {
        let drawn = self.reading_chars(index)?;
        let mut boxes = Vec::with_capacity(drawn.len());
        let mut line_number: u32 = 0;
        for (block_number, block) in (0u32..).zip(page_blocks(&drawn)) {
            for line in block {
                for i in line {
                    boxes.push(TextBox {
                        ch: drawn[i].ch,
                        rect: drawn[i].rect,
                        line: line_number,
                        block: block_number,
                    });
                }
                line_number += 1;
            }
        }
        Ok(boxes)
    }

    /// Tout le texte de la page `index`, dans l'ordre de lecture (export texte) : le même que
    /// celui de la sélection et de la copie.
    pub fn page_text(&self, index: u16) -> CoreResult<String> {
        let drawn = self.reading_chars(index)?;
        let chars: Vec<char> = drawn.iter().map(|d| d.ch).collect();
        let rects: Vec<Rect> = drawn.iter().map(|d| d.rect).collect();
        Ok(crate::reading_order::blocks_text(
            &page_blocks(&drawn),
            &chars,
            &rects,
        ))
    }

    /// Les caractères que lit la couche de texte : ceux de [`Document::drawn_chars`], où chaque
    /// espace **ajoutée par PDFium** entre deux mots reçoit l'emprise du blanc qu'elle
    /// représente (voir [`with_word_spaces`]). Le contrôle après caviardage, lui, garde
    /// `drawn_chars` : une espace reconstituée n'est pas du texte du fichier.
    pub(crate) fn reading_chars(&self, index: u16) -> CoreResult<Vec<DrawnChar>> {
        let mut drawn = with_word_spaces(self.drawn_chars(index)?);
        add_rtl_word_spaces(&mut drawn);
        Ok(drawn)
    }

    /// Les caractères de la page dans l'ordre où le fichier les dessine, en espace affiché.
    /// Seuls les sauts de ligne ajoutés par PDFium (`\r`, `\n`) sont écartés : un caractère
    /// sans largeur reste là (le contrôle après caviardage doit le voir, même invisible).
    ///
    /// Si PDFium n'expose aucun caractère alors que la page porte un objet texte, les
    /// caractères viennent du repli [`Document::tounicode_fallback`] (audit L2). Le contrôle
    /// après caviardage en profite : un texte que PDFium ne voit pas, mais que les autres
    /// lecteurs extraient, compte aussi comme du texte resté sous la zone.
    pub(crate) fn drawn_chars(&self, index: u16) -> CoreResult<Vec<DrawnChar>> {
        let (chars, fallback) = self.pdfium_chars(index)?;
        if fallback {
            return self.tounicode_fallback(index);
        }
        Ok(chars)
    }

    /// Les caractères qu'expose PDFium (voir [`Document::drawn_chars`]), et vrai s'il faut
    /// recourir au repli : aucun caractère, mais un objet texte sur la page.
    fn pdfium_chars(&self, index: u16) -> CoreResult<(Vec<DrawnChar>, bool)> {
        let _guard = engine::guard();
        let page = self
            .inner()
            .pages()
            .get(self.checked_index_unlocked(index)?)?;
        let text = page.text()?;
        if text.len() <= 0 {
            return Ok((Vec::new(), draws_text(&page)));
        }
        let frame = crate::geometry::frame_of(&page);
        // Les unités UTF-16 que PDFium rend, avec leur emprise, et à part leur angle à l'écran.
        // Une unité sans emprise est ignorée.
        let capacity = usize::try_from(text.len()).unwrap_or(0);
        let mut units: Vec<(u32, Rect)> = Vec::with_capacity(capacity);
        let mut angles: Vec<f32> = Vec::with_capacity(capacity);
        for c in text.chars().iter() {
            let Ok(bounds) = c.loose_bounds() else {
                continue;
            };
            units.push((
                c.unicode_value(),
                frame.from_user_space(Rect::from_pdf(bounds)),
            ));
            angles.push(screen_angle(c.angle_degrees().unwrap_or(0.0), frame.rotate));
        }
        // Une paire de substitution (deux unités) donne un seul caractère, à l'angle de sa
        // première unité. Un caractère sans valeur Unicode (glyphe de contrôle) est ignoré, ainsi
        // que les sauts de ligne ajoutés par PDFium.
        let chars = pair_surrogates(&units)
            .into_iter()
            .filter(|(ch, _, _)| *ch != '\r' && *ch != '\n')
            .map(|(ch, rect, first)| {
                let angle = angles[first];
                DrawnChar {
                    ch,
                    rect,
                    horizontal: is_horizontal(angle),
                    angle,
                }
            })
            .collect();
        Ok((chars, false))
    }

    /// Repli de la couche de texte (audit L2) : les caractères de la page `index` retrouvés
    /// par les `/ToUnicode` de ses polices, lues avec `lopdf` (module `tounicode`, limites
    /// dans sa documentation), en espace affiché et dans l'ordre de dessin. Le document lu est
    /// le document **courant**, gardé par [`Document::lopdf_view`] (qui prend le jeton PDFium :
    /// jamais sous `engine::guard()`).
    ///
    /// Un fichier que `lopdf` ne sait pas lire ne donne aucun caractère : c'est un repli, il
    /// n'ajoute pas de cas d'erreur. De même si `lopdf` et PDFium ne comptent pas le même
    /// nombre de pages (arbre de pages abîmé) : les numéros ne désigneraient peut-être pas la
    /// même page, on ne devine pas.
    pub(crate) fn tounicode_fallback(&self, index: u16) -> CoreResult<Vec<DrawnChar>> {
        let frame = self.page_frame(index)?;
        let page_count = usize::from(self.page_count());
        let Some(doc) = self.lopdf_view() else {
            return Ok(Vec::new());
        };
        let pages = doc.get_pages();
        if pages.len() != page_count {
            return Ok(Vec::new());
        }
        let Some(&page_id) = pages.get(&(u32::from(index) + 1)) else {
            return Ok(Vec::new());
        };
        Ok(tounicode::page_chars(doc, page_id)
            .into_iter()
            .filter(|c| c.ch != '\r' && c.ch != '\n')
            .map(|c| {
                let angle = screen_angle(c.angle, frame.rotate);
                DrawnChar {
                    ch: c.ch,
                    rect: frame.from_user_space(c.rect),
                    horizontal: is_horizontal(angle),
                    angle,
                }
            })
            .collect())
    }

    /// Cherche `query` dans la page `index`. Une requête vide ne trouve rien.
    ///
    /// La recherche parcourt le texte **dans l'ordre de lecture** ([`Document::text_boxes`]),
    /// comme la sélection et la copie : un mot dessiné à part, une valeur dessinée après son
    /// libellé ou une expression qui passe à la ligne sont trouvés (audit du Lot 13, C5 ; la
    /// recherche de PDFium suivait l'ordre de dessin). Un changement de ligne compte comme une
    /// espace, et plusieurs espaces comme une seule.
    pub fn search_page(
        &self,
        index: u16,
        query: &str,
        options: &SearchOptions,
    ) -> CoreResult<Vec<SearchHit>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let boxes = self.text_boxes(index)?;
        Ok(find_in_boxes(&boxes, query, options)
            .into_iter()
            .map(|rects| SearchHit { page: index, rects })
            .collect())
    }
}

/// Nombre maximal d'objets examinés par [`draws_text`] à l'intérieur des formulaires.
const MAX_FORM_OBJECTS: usize = 10_000;

/// Vrai si la page porte un objet texte, à son niveau ou dans un formulaire XObject (jusqu'à
/// `tounicode::MAX_FORM_DEPTH` niveaux) : sans objet texte, inutile de relire le fichier avec
/// `lopdf` (une page numérisée n'a que des images). Au-delà de [`MAX_FORM_OBJECTS`] objets,
/// on répond oui par prudence : le repli, lui, est borné.
fn draws_text(page: &PdfPage<'_>) -> bool {
    // Pile explicite (pas de récursion) : formulaires à examiner, avec leur profondeur.
    let mut pending: Vec<(PdfPageObject<'_>, usize)> = Vec::new();
    for object in page.objects().iter() {
        match object {
            PdfPageObject::Text(_) => return true,
            PdfPageObject::XObjectForm(_) => pending.push((object, 1)),
            _ => {}
        }
    }
    let mut examined = 0usize;
    while let Some((object, depth)) = pending.pop() {
        let PdfPageObject::XObjectForm(form) = &object else {
            continue;
        };
        for i in 0..form.len() {
            examined += 1;
            if examined > MAX_FORM_OBJECTS {
                return true;
            }
            match form.get(i) {
                Ok(PdfPageObject::Text(_)) => return true,
                Ok(child @ PdfPageObject::XObjectForm(_)) if depth < tounicode::MAX_FORM_DEPTH => {
                    pending.push((child, depth + 1));
                }
                _ => {}
            }
        }
    }
    false
}

/// L'angle à l'écran (0 à 360°) d'un caractère dont PDFium donne l'angle `user` en espace
/// utilisateur, sur une page tournée de `rotate` degrés (`/Rotate`).
///
/// `FPDFText_GetCharAngle` mesure atan2(-b, a) de la matrice du caractère, c'est-à-dire dans
/// le sens **horaire** (un texte qui monte vaut 270°), et `/Rotate` tourne l'affichage dans le
/// sens horaire lui aussi : les deux s'ajoutent.
fn screen_angle(user: f32, rotate: u16) -> f32 {
    (user + f32::from(rotate)).rem_euclid(360.0)
}

/// Vrai pour un angle à l'écran « horizontal », à [`ANGLE_TOLERANCE`] près.
fn is_horizontal(angle: f32) -> bool {
    angle <= ANGLE_TOLERANCE || angle >= 360.0 - ANGLE_TOLERANCE
}

/// Vrai pour une moitié haute de paire de substitution UTF-16 (`D800`–`DBFF`).
fn is_high_surrogate(unit: u32) -> bool {
    (0xD800..=0xDBFF).contains(&unit)
}

/// Vrai pour une moitié basse de paire de substitution UTF-16 (`DC00`–`DFFF`).
fn is_low_surrogate(unit: u32) -> bool {
    (0xDC00..=0xDFFF).contains(&unit)
}

/// Recompose les caractères à partir des unités UTF-16 que rend PDFium (audit L1).
///
/// Sous Windows, PDFium donne un caractère hors du plan multilingue de base (emoji, lettres
/// mathématiques `𝜋`, idéogrammes de l'extension B) en **deux** unités : une moitié haute
/// (`D800`–`DBFF`) puis une moitié basse (`DC00`–`DFFF`). Chacune est jetée par
/// `char::from_u32`, d'où un texte, une recherche et une copie amputés. Une paire devient un
/// seul caractère dont l'emprise est l'**union** des deux rectangles ; une moitié isolée est
/// écartée (aucun caractère valide) ; toute autre unité passe telle quelle, et celle qui n'est
/// pas un caractère Unicode est écartée comme avant. Chaque caractère rendu porte l'index de
/// sa première unité dans `units` : l'appelant y retrouve ce qu'il garde à part (l'angle).
///
/// \[Apprentissage\] Un `char` Rust est une *valeur scalaire* Unicode : les points de code
/// `D800`–`DFFF` en sont exclus, donc `char::from_u32(0xD835)` vaut `None`. Ces points ne
/// servent qu'à UTF-16, qui écrit un point de code au-delà de `FFFF` en deux unités de 16 bits
/// (voir `docs/apprentissage/`).
pub(crate) fn pair_surrogates(units: &[(u32, Rect)]) -> Vec<(char, Rect, usize)> {
    let mut out = Vec::with_capacity(units.len());
    let mut i = 0;
    while i < units.len() {
        let (unit, rect) = units[i];
        if is_high_surrogate(unit) {
            if let Some(&(low, low_rect)) = units.get(i + 1).filter(|u| is_low_surrogate(u.0)) {
                let code = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
                if let Some(ch) = char::from_u32(code) {
                    out.push((ch, union(rect, low_rect), i));
                }
                i += 2;
                continue;
            }
        } else if let Some(ch) = char::from_u32(unit) {
            out.push((ch, rect, i));
        }
        // Moitié isolée (haute sans basse, ou basse seule) : ignorée.
        i += 1;
    }
    out
}

/// Plus petit rectangle qui contient `a` et `b`.
fn union(a: Rect, b: Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    Rect {
        x,
        y,
        w: (a.x + a.w).max(b.x + b.w) - x,
        h: (a.y + a.h).max(b.y + b.h) - y,
    }
}

/// Donne une emprise aux espaces **sans emprise** que PDFium ajoute entre deux mots
/// (relecture de la PR B du Lot 13).
///
/// Beaucoup de fichiers (pdfTeX, donc LaTeX) séparent les mots par un simple décalage
/// (crénage `TJ`), sans caractère espace. PDFium ajoute alors une espace de largeur nulle,
/// que l'ordre de lecture écartait : « Helloworld » à la copie, à l'export et à la recherche.
/// Chaque espace sans emprise entre deux caractères horizontaux de la même ligne, séparés par
/// un blanc de mot (au plus une hauteur de caractère), reçoit le rectangle de ce blanc.
fn with_word_spaces(mut chars: Vec<DrawnChar>) -> Vec<DrawnChar> {
    for i in 1..chars.len().saturating_sub(1) {
        let empty = chars[i].rect.w <= 0.0 || chars[i].rect.h <= 0.0;
        if !(empty && chars[i].ch.is_whitespace()) {
            continue;
        }
        let (prev, next) = (&chars[i - 1], &chars[i + 1]);
        let same_line = prev.horizontal
            && next.horizontal
            && (prev.rect.y - next.rect.y).abs() < 0.5 * prev.rect.h.max(next.rect.h);
        let left = prev.rect.x + prev.rect.w;
        // Un blanc de mot, pas une gouttière de colonnes ni une tabulation : au plus une
        // hauteur de caractère (un espacement de mots courant vaut environ un tiers).
        let gap = next.rect.x - left;
        if !same_line || gap <= 0.0 || gap > prev.rect.h || prev.rect.h <= 0.0 {
            continue;
        }
        chars[i].rect = Rect {
            x: left,
            y: prev.rect.y,
            w: next.rect.x - left,
            h: prev.rect.h,
        };
        chars[i].horizontal = true;
    }
    chars
}

/// Ajoute une espace entre les mots d'une ligne de droite à gauche qui n'en a aucune (audit du
/// Lot 14, L3) : PDFium ne déduit les espaces des blancs que pour l'écriture de gauche à
/// droite, et poppler, MuPDF et pdf.js séparent les mots. `reading_order::word_gaps` décide où ;
/// l'espace reçoit l'emprise du blanc et est **ajoutée en fin de liste** : l'ordre de lecture
/// est recalculé par la géométrie, elle retrouve donc sa place entre les deux mots.
///
/// \[Apprentissage\] On collecte d'abord les ajouts (emprunt partagé de `drawn`), puis on les
/// pousse : on ne peut pas modifier un `Vec` pendant qu'on le parcourt.
fn add_rtl_word_spaces(drawn: &mut Vec<DrawnChar>) {
    let chars: Vec<char> = drawn.iter().map(|d| d.ch).collect();
    let rects: Vec<Rect> = drawn
        .iter()
        .map(|d| {
            if d.horizontal {
                d.rect
            } else {
                Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 0.0,
                    h: 0.0,
                }
            }
        })
        .collect();
    let mut spaces = Vec::new();
    for line in crate::reading_order::layout_chars(&rects, &chars)
        .iter()
        .flatten()
    {
        for i in crate::reading_order::word_gaps(line, &rects, &chars) {
            let (a, b) = (&rects[line[i]], &rects[line[i + 1]]);
            let (left, right) = if a.x <= b.x { (a, b) } else { (b, a) };
            let x = left.x + left.w;
            spaces.push(DrawnChar {
                ch: ' ',
                rect: Rect {
                    x,
                    y: left.y.min(right.y),
                    w: right.x - x,
                    h: left.h.max(right.h),
                },
                horizontal: true,
                angle: 0.0,
            });
        }
    }
    drawn.extend(spaces);
}

/// Un caractère du texte cherché, et la boîte d'où il vient (`None` : espace ajoutée entre
/// deux lignes ou deux mots sans caractère d'espace).
type Searchable = (char, Option<usize>);

/// `c` comparable : minuscule si la casse est ignorée, toute espace devient `' '`.
fn fold(c: char, match_case: bool) -> char {
    if c.is_whitespace() {
        ' '
    } else if match_case {
        c
    } else {
        c.to_lowercase().next().unwrap_or(c)
    }
}

/// Le texte des boîtes tel que la recherche le voit : un changement de ligne, ou un grand
/// blanc sans caractère d'espace (un libellé et sa valeur), donne une espace ; plusieurs
/// espaces de suite n'en font qu'une (mêmes règles que `reading_order::blocks_text`).
fn searchable(boxes: &[TextBox], match_case: bool) -> Vec<Searchable> {
    let mut out: Vec<Searchable> = Vec::with_capacity(boxes.len());
    for (i, b) in boxes.iter().enumerate() {
        if let Some(p) = i.checked_sub(1).map(|p| &boxes[p]) {
            let (a, r) = (&p.rect, &b.rect);
            let gap = (r.x - (a.x + a.w)).max(a.x - (r.x + r.w));
            let separated = p.line != b.line || gap > 0.5 * a.h.max(r.h);
            if separated && out.last().is_some_and(|(c, _)| *c != ' ') {
                out.push((' ', None));
            }
        }
        let c = fold(b.ch, match_case);
        if c == ' ' && out.last().is_some_and(|(last, _)| *last == ' ') {
            continue;
        }
        out.push((c, Some(i)));
    }
    out
}

/// Les occurrences de `query` dans les boîtes, chacune en rectangles (un par ligne touchée).
/// Fonction pure, sans PDFium.
fn find_in_boxes(boxes: &[TextBox], query: &str, options: &SearchOptions) -> Vec<Vec<Rect>> {
    let mut needle: Vec<char> = Vec::new();
    for c in query.trim().chars().map(|c| fold(c, options.match_case)) {
        if !(c == ' ' && needle.last() == Some(&' ')) {
            needle.push(c);
        }
    }
    let hay = searchable(boxes, options.match_case);
    let is_word = |k: usize| hay.get(k).is_some_and(|(c, _)| c.is_alphanumeric());
    let mut hits = Vec::new();
    let mut start = 0;
    while !needle.is_empty() && start + needle.len() <= hay.len() {
        let end = start + needle.len();
        let matches = hay[start..end]
            .iter()
            .zip(&needle)
            .all(|((c, _), n)| c == n);
        let whole = !options.whole_word || (!(start > 0 && is_word(start - 1)) && !is_word(end));
        if matches && whole {
            hits.push(line_rects(
                boxes,
                hay[start..end].iter().filter_map(|(_, i)| *i),
            ));
            start = end;
        } else {
            start += 1;
        }
    }
    hits
}

/// Rectangles qui couvrent les boîtes `indices` : un par ligne (union des boîtes).
fn line_rects(boxes: &[TextBox], indices: impl Iterator<Item = usize>) -> Vec<Rect> {
    let mut rects: Vec<Rect> = Vec::new();
    let mut current_line = None;
    for i in indices {
        let r = boxes[i].rect;
        match rects.last_mut() {
            Some(last) if current_line == Some(boxes[i].line) => {
                let right = (last.x + last.w).max(r.x + r.w);
                let top = (last.y + last.h).max(r.y + r.h);
                last.x = last.x.min(r.x);
                last.y = last.y.min(r.y);
                last.w = right - last.x;
                last.h = top - last.y;
            }
            _ => {
                rects.push(r);
                current_line = Some(boxes[i].line);
            }
        }
    }
    rects
}

/// Les blocs de lecture de la page : le texte horizontal ordonné par `reading_order::layout`,
/// puis chaque suite de texte tourné (bandeau vertical, en-tête de tableau tourné) dans son
/// ordre de dessin, en blocs d'une ligne. Découper du texte tourné en lignes horizontales le
/// rendrait illisible (relecture du correctif 0.12.2).
fn page_blocks(drawn: &[DrawnChar]) -> Vec<crate::reading_order::Block> {
    let hidden = Rect {
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    };
    let chars: Vec<char> = drawn.iter().map(|d| d.ch).collect();
    let mut horizontal: Vec<Rect> = drawn
        .iter()
        .map(|d| if d.horizontal { d.rect } else { hidden })
        .collect();
    // Colonnes d'écriture verticale (CJK) : retirées du découpage en lignes horizontales, lues
    // à part dans leur ordre de dessin (audit du Lot 13, C4).
    let vertical = crate::reading_order::vertical_runs(&horizontal, &chars);
    for &i in vertical.iter().flatten() {
        horizontal[i] = hidden;
    }
    let mut blocks = crate::reading_order::layout_chars(&horizontal, &chars);
    blocks.extend(vertical.into_iter().map(|run| vec![run]));
    let mut run: Vec<usize> = Vec::new();
    let mut run_angle = 0.0;
    for (i, d) in drawn.iter().enumerate() {
        let visible = !d.horizontal && d.rect.w > 0.0 && d.rect.h > 0.0;
        let same_run = visible && !run.is_empty() && (d.angle - run_angle).abs() <= ANGLE_TOLERANCE;
        if !same_run && !run.is_empty() {
            blocks.push(vec![std::mem::take(&mut run)]);
        }
        if visible {
            if run.is_empty() {
                run_angle = d.angle;
            }
            run.push(i);
        }
    }
    if !run.is_empty() {
        blocks.push(vec![run]);
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f32) -> Rect {
        Rect {
            x,
            y: 0.0,
            w: 10.0,
            h: 12.0,
        }
    }

    #[test]
    fn pair_surrogates_recomposes_a_pair_and_drops_a_lone_half() {
        let joined = Rect {
            x: 0.0,
            y: 0.0,
            w: 20.0,
            h: 12.0,
        };
        let pair = pair_surrogates(&[(0xD835, r(0.0)), (0xDF0B, r(10.0))]);
        assert_eq!(pair, vec![('\u{1D70B}', joined, 0)]);
        let lone = pair_surrogates(&[(0xD835, r(0.0)), (0x41, r(10.0))]);
        assert_eq!(lone, vec![('A', r(10.0), 1)]);
        let stray = pair_surrogates(&[(0xDF0B, r(0.0)), (0x42, r(10.0)), (0xD835, r(20.0))]);
        assert_eq!(stray, vec![('B', r(10.0), 1)]);
    }

    /// Deux moitiés hautes de suite : la première est isolée (écartée), la seconde forme la
    /// paire avec la basse qui suit ; une moitié haute en fin de texte est écartée. L'index
    /// rendu est celui de la première unité du caractère (l'appelant y lit l'angle).
    #[test]
    fn pair_surrogates_with_two_high_halves_and_a_trailing_one() {
        let joined = Rect {
            x: 10.0,
            y: 0.0,
            w: 20.0,
            h: 12.0,
        };
        let two_high = pair_surrogates(&[(0xD835, r(0.0)), (0xD835, r(10.0)), (0xDF0B, r(20.0))]);
        assert_eq!(two_high, vec![('\u{1D70B}', joined, 1)]);
        let trailing = pair_surrogates(&[(0x41, r(0.0)), (0xD835, r(10.0))]);
        assert_eq!(trailing, vec![('A', r(0.0), 0)]);
    }

    /// L'angle d'un caractère se mesure dans le sens horaire (`FPDFText_GetCharAngle`) et
    /// `/Rotate` tourne l'affichage dans le sens horaire : les deux s'**ajoutent**. Un texte
    /// qui monte (`0 1 -1 0 Tm`) sur une page `/Rotate 90` se lit de gauche à droite à l'écran.
    #[test]
    fn a_rising_text_on_a_page_rotated_by_90_is_horizontal() {
        let rising = |doc: &Document| -> Vec<(f32, bool)> {
            let chars = doc.drawn_chars(0).unwrap();
            chars
                .iter()
                .filter(|c| c.ch == 'X')
                .map(|c| (c.angle, c.horizontal))
                .collect()
        };
        let plain = Document::from_bytes(crate::testing::sidebar_pdf(), None).unwrap();
        assert_eq!(rising(&plain), vec![(270.0, false)], "monte à l'écran");
        let mut lo = lopdf::Document::load_mem(&crate::testing::sidebar_pdf()).unwrap();
        lo.get_dictionary_mut((3, 0)).unwrap().set("Rotate", 90);
        let mut out = Vec::new();
        lo.save_to(&mut out).unwrap();
        let rotated = Document::from_bytes(out, None).unwrap();
        assert_eq!(rising(&rotated), vec![(0.0, true)], "horizontal à l'écran");
    }

    fn drawn(ch: char, x: f32, y: f32) -> DrawnChar {
        DrawnChar {
            ch,
            rect: Rect {
                x,
                y,
                w: 12.0,
                h: 12.0,
            },
            horizontal: true,
            angle: 0.0,
        }
    }

    /// C4 (audit du Lot 13) : une colonne verticale japonaise reste **un** bloc d'une ligne, lu
    /// de haut en bas, au lieu d'une ligne par caractère ; le texte horizontal ne change pas.
    #[test]
    fn a_vertical_cjk_column_is_one_block_read_top_down() {
        let mut page: Vec<DrawnChar> = "縦書きです"
            .chars()
            .enumerate()
            .map(|(k, c)| drawn(c, 500.0, 700.0 - 13.0 * k as f32))
            .collect();
        page.extend(
            "Titre"
                .chars()
                .enumerate()
                .map(|(k, c)| drawn(c, 100.0 + 7.0 * k as f32, 760.0)),
        );
        let blocks = page_blocks(&page);
        let lines: Vec<String> = blocks
            .iter()
            .flatten()
            .map(|line| line.iter().map(|&i| page[i].ch).collect())
            .collect();
        assert!(lines.contains(&"縦書きです".to_string()), "{lines:?}");
        assert!(lines.contains(&"Titre".to_string()), "{lines:?}");
        assert_eq!(lines.len(), 2, "{lines:?}");
    }
}
