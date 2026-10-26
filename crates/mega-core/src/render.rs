// SPDX-License-Identifier: GPL-3.0-or-later
//! Rendu d'une page en image.

use image::ImageEncoder;
use pdfium_render::prelude::{PdfRenderConfig, Pixels};

use crate::document::Document;
use crate::error::{CoreError, CoreResult};

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
        let page = self.inner().pages().get(self.checked_index(index)?)?;
        let width_px = (page.width().value * scale).round().max(1.0) as Pixels;
        let height_px = (page.height().value * scale).round().max(1.0) as Pixels;
        let config = PdfRenderConfig::new()
            .set_target_size(width_px, height_px)
            .render_form_data(true);
        let bitmap = page.render_with_config(&config)?;
        Ok(RenderedPage {
            width_px: bitmap.width() as u32,
            height_px: bitmap.height() as u32,
            rgba: bitmap.as_rgba_bytes(),
        })
    }
}
