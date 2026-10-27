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
/// L'ordre des `TextBox` est l'ordre de lecture déterminé par PDFium : le frontend s'en
/// sert pour la sélection (plage d'index) et la copie.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextBox {
    /// Le caractère.
    pub ch: char,
    /// Son emprise approximative (`loose_bounds` : inclut la hauteur de ligne).
    pub rect: Rect,
}

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
    /// Tous les caractères de la page `index`, dans l'ordre de lecture.
    pub fn text_boxes(&self, index: u16) -> CoreResult<Vec<TextBox>> {
        let _guard = engine::guard();
        let page = self
            .inner()
            .pages()
            .get(self.checked_index_unlocked(index)?)?;
        let text = page.text()?;
        let frame = crate::geometry::frame_of(&page);
        let mut boxes = Vec::with_capacity(text.len().max(0) as usize);
        for c in text.chars().iter() {
            // Un caractère sans valeur Unicode (glyphe de contrôle) ou sans emprise est ignoré.
            let (Some(ch), Ok(bounds)) = (c.unicode_char(), c.loose_bounds()) else {
                continue;
            };
            boxes.push(TextBox {
                ch,
                rect: frame.from_user_space(Rect::from_pdf(bounds)),
            });
        }
        Ok(boxes)
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
