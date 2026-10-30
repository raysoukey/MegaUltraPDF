// SPDX-License-Identifier: GPL-3.0-or-later
//! Couche de texte d'une page (caractères positionnés) et recherche.

use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::engine;
use crate::error::CoreResult;
use crate::geometry::Rect;

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
        let drawn = self.drawn_chars(index)?;
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
        let drawn = self.drawn_chars(index)?;
        let chars: Vec<char> = drawn.iter().map(|d| d.ch).collect();
        let rects: Vec<Rect> = drawn.iter().map(|d| d.rect).collect();
        Ok(crate::reading_order::blocks_text(
            &page_blocks(&drawn),
            &chars,
            &rects,
        ))
    }

    /// Les caractères de la page dans l'ordre où le fichier les dessine, en espace affiché.
    /// Seuls les sauts de ligne ajoutés par PDFium (`\r`, `\n`) sont écartés : un caractère
    /// sans largeur reste là (le contrôle après caviardage doit le voir, même invisible).
    pub(crate) fn drawn_chars(&self, index: u16) -> CoreResult<Vec<DrawnChar>> {
        let _guard = engine::guard();
        let page = self
            .inner()
            .pages()
            .get(self.checked_index_unlocked(index)?)?;
        let text = page.text()?;
        let frame = crate::geometry::frame_of(&page);
        let mut chars = Vec::with_capacity(text.len().max(0) as usize);
        for c in text.chars().iter() {
            // Un caractère sans valeur Unicode (glyphe de contrôle) ou sans emprise est ignoré.
            let (Some(ch), Ok(bounds)) = (c.unicode_char(), c.loose_bounds()) else {
                continue;
            };
            if ch == '\r' || ch == '\n' {
                continue;
            }
            // PDFium mesure l'angle dans l'espace utilisateur, sens trigonométrique ; `/Rotate`
            // tourne l'affichage dans le sens horaire.
            let angle =
                (c.angle_degrees().unwrap_or(0.0) - f32::from(frame.rotate)).rem_euclid(360.0);
            let horizontal = angle <= ANGLE_TOLERANCE || angle >= 360.0 - ANGLE_TOLERANCE;
            chars.push(DrawnChar {
                ch,
                rect: frame.from_user_space(Rect::from_pdf(bounds)),
                horizontal,
                angle,
            });
        }
        Ok(chars)
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
