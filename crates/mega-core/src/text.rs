// SPDX-License-Identifier: GPL-3.0-or-later
//! Couche de texte d'une page (caractères positionnés) et recherche.

use pdfium_render::prelude::{PdfSearchDirection, PdfSearchOptions};
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
    pub fn search_page(
        &self,
        index: u16,
        query: &str,
        options: &SearchOptions,
    ) -> CoreResult<Vec<SearchHit>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let _guard = engine::guard();
        let page = self
            .inner()
            .pages()
            .get(self.checked_index_unlocked(index)?)?;
        let text = page.text()?;
        let pdf_options = PdfSearchOptions::new()
            .match_case(options.match_case)
            .match_whole_word(options.whole_word);
        let search = text.search(query, &pdf_options)?;
        let frame = crate::geometry::frame_of(&page);
        let hits = search
            .iter(PdfSearchDirection::SearchForward)
            .map(|segments| SearchHit {
                page: index,
                rects: segments
                    .iter()
                    .map(|s| frame.from_user_space(Rect::from_pdf(s.bounds())))
                    .collect(),
            })
            .collect();
        Ok(hits)
    }
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
    let horizontal: Vec<Rect> = drawn
        .iter()
        .map(|d| if d.horizontal { d.rect } else { hidden })
        .collect();
    let mut blocks = crate::reading_order::layout(&horizontal);
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
