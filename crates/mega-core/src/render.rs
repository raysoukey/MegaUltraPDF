// SPDX-License-Identifier: GPL-3.0-or-later
//! Rendu d'une page en image.

use image::ImageEncoder;
use pdfium_render::prelude::{PdfRenderConfig, Pixels};

use crate::document::Document;
use crate::error::{CoreError, CoreResult};
use crate::geometry::Rotation;

/// Une page rendue en pixels RGBA (4 octets par pixel, lignes de haut en bas).
#[derive(Debug, Clone)]
pub struct RenderedPage {
    /// Largeur en pixels.
    pub width_px: u32,
    /// Hauteur en pixels.
    pub height_px: u32,
    /// `width_px * height_px * 4` octets.
    pub rgba: Vec<u8>,
}

impl RenderedPage {
    /// Encode la page en PNG (format servi au frontend par le protocole `mega://`).
    pub fn to_png(&self) -> CoreResult<Vec<u8>> {
        let mut out = Vec::new();
        image::codecs::png::PngEncoder::new(&mut out)
            .write_image(
                &self.rgba,
                self.width_px,
                self.height_px,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|e| CoreError::Pdfium(format!("encodage PNG : {e}")))?;
        Ok(out)
    }
}

impl Document {
    /// Rend la page `index` à l'échelle `scale` (1.0 = 72 pixels par pouce).
    ///
    /// La taille en pixels est `round(points × scale)` : à l'échelle 2, une page A4
    /// (595 × 842 pt) donne 1190 × 1684 px.
    pub fn render_page(&self, index: u16, scale: f32) -> CoreResult<RenderedPage> {
        self.render_page_with(index, scale, Rotation::None)
    }

    /// Comme [`render_page`](Self::render_page), avec une rotation d'affichage.
    ///
    /// Pour un quart de tour, largeur et hauteur de l'image sont échangées.
    pub fn render_page_with(
        &self,
        index: u16,
        scale: f32,
        rotation: Rotation,
    ) -> CoreResult<RenderedPage> {
        self.render_page_opts(index, scale, rotation, true)
    }

    /// Rendu complet, avec ou sans annotations et champs (export « page nue »).
    pub(crate) fn render_page_opts(
        &self,
        index: u16,
        scale: f32,
        rotation: Rotation,
        annotations: bool,
    ) -> CoreResult<RenderedPage> {
        let _guard = crate::engine::guard();
        let page = self
            .inner()
            .pages()
            .get(self.checked_index_unlocked(index)?)?;
        let (page_w, page_h) = if rotation.swaps_axes() {
            (page.height().value, page.width().value)
        } else {
            (page.width().value, page.height().value)
        };
        let width = (page_w * scale).round().max(1.0);
        let height = (page_h * scale).round().max(1.0);
        if !width.is_finite()
            || !height.is_finite()
            || (width as u64).saturating_mul(height as u64) > crate::error::MAX_RENDER_PIXELS
        {
            return Err(CoreError::RenderTooLarge {
                width_px: width as u64,
                height_px: height as u64,
            });
        }
        let width_px = width as Pixels;
        let height_px = height as Pixels;
        let config = PdfRenderConfig::new()
            .set_target_size(width_px, height_px)
            .rotate(rotation.to_pdfium(), false)
            .render_form_data(annotations)
            // Dessine les annotations (drapeau FPDF_ANNOT) : surlignages, formes, tampons…
            .render_annotations(annotations);
        let bitmap = page.render_with_config(&config)?;
        Ok(RenderedPage {
            width_px: bitmap.width() as u32,
            height_px: bitmap.height() as u32,
            rgba: bitmap.as_rgba_bytes(),
        })
    }
}
