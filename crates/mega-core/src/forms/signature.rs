// SPDX-License-Identifier: GPL-3.0-or-later
//! Signature visuelle : une image ou des traits ajoutés au **contenu** de la page.
//!
//! Ce n'est pas un champ `/Sig` (signature cryptographique, Lot 7) : c'est un dessin
//! permanent, visible dans tous les lecteurs, que personne ne pourra « décocher ».
//!
//! Le chargement borné de l'image (entrée non fiable) et son placement sont partagés avec
//! l'insertion d'images du Lot 5 : voir `crate::content::image`.

use pdfium_render::prelude::{
    PdfColor, PdfPageContentRegenerationStrategy, PdfPageObjectCommon, PdfPageObjectLineCap,
    PdfPageObjectLineJoin, PdfPageObjectsCommon, PdfPagePathObject, PdfPoints,
};
use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::engine;
use crate::error::{CoreError, CoreResult};
use crate::geometry::{Rect, frame_of};

/// Épaisseur du trait d'une signature dessinée (points).
const STROKE_WIDTH: f32 = 2.0;

/// D'où vient la signature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SignatureSource {
    /// Un fichier PNG ou JPEG.
    Image {
        /// Chemin du fichier.
        path: String,
    },
    /// Des traits (points PDF, déjà placés sur la page).
    Strokes {
        /// Une liste de points par trait.
        strokes: Vec<Vec<[f32; 2]>>,
    },
}

fn invalid(msg: String) -> CoreError {
    CoreError::InvalidOperation(msg)
}

/// Ajuste `(w, h)` dans `rect` en gardant les proportions, centré (voir `content::image`).
pub(crate) use crate::content::image::fit;

impl Document {
    /// Appose une signature visuelle dans `rect` (points PDF, espace **affiché**) sur la
    /// page `page`. Sur une page tournée, l'image reste droite à l'écran et les traits
    /// suivent les points désignés.
    pub fn insert_signature(
        &mut self,
        page: u16,
        rect: Rect,
        source: &SignatureSource,
    ) -> CoreResult<()> {
        if !(rect.w > 0.0 && rect.h > 0.0 && rect.w.is_finite() && rect.h.is_finite()) {
            return Err(invalid("la zone de signature est vide".to_string()));
        }
        let size = self.page_size(page)?;
        if rect.x < -1.0
            || rect.y < -1.0
            || rect.x + rect.w > size.width_pt + 1.0
            || rect.y + rect.h > size.height_pt + 1.0
        {
            return Err(invalid("la zone de signature sort de la page".to_string()));
        }
        let inside = |p: &[f32; 2]| {
            p[0].is_finite()
                && p[1].is_finite()
                && p[0] >= rect.x - 1.0
                && p[0] <= rect.x + rect.w + 1.0
                && p[1] >= rect.y - 1.0
                && p[1] <= rect.y + rect.h + 1.0
        };
        match source {
            SignatureSource::Image { path } => {
                let image = crate::content::image::load_bounded(path)?;
                let placed = fit(image.width() as f32, image.height() as f32, &rect);
                let _guard = engine::guard();
                let mut pdf_page = self
                    .inner()
                    .pages()
                    .get(self.checked_index_unlocked(page)?)?;
                let frame = frame_of(&pdf_page);
                crate::content::image::place(&mut pdf_page, &frame, &image, &placed)
            }
            SignatureSource::Strokes { strokes } => {
                let valid: Vec<&Vec<[f32; 2]>> = strokes.iter().filter(|s| s.len() >= 2).collect();
                if valid.is_empty() {
                    return Err(invalid(
                        "une signature dessinée exige au moins un trait".to_string(),
                    ));
                }
                if valid.iter().any(|s| s.iter().any(|p| !inside(p))) {
                    return Err(invalid("un trait sort de la zone de signature".to_string()));
                }
                let _guard = engine::guard();
                let page_index = self.checked_index_unlocked(page)?;
                let doc = self.inner();
                let mut pdf_page = doc.pages().get(page_index)?;
                let frame = frame_of(&pdf_page);
                // Un seul flux de contenu pour toute la signature (sinon pdfium-render
                // régénère le contenu à chaque objet ajouté : un flux par trait).
                pdf_page
                    .set_content_regeneration_strategy(PdfPageContentRegenerationStrategy::Manual);
                let black = PdfColor::new(0, 0, 0, 255);
                for stroke in valid {
                    let start = frame.point_to_user_space(stroke[0]);
                    let mut obj = PdfPagePathObject::new(
                        doc,
                        PdfPoints::new(start[0]),
                        PdfPoints::new(start[1]),
                        Some(black),
                        Some(PdfPoints::new(STROKE_WIDTH)),
                        None,
                    )?;
                    for p in &stroke[1..] {
                        let p = frame.point_to_user_space(*p);
                        obj.line_to(PdfPoints::new(p[0]), PdfPoints::new(p[1]))?;
                    }
                    obj.set_line_cap(PdfPageObjectLineCap::Round)?;
                    obj.set_line_join(PdfPageObjectLineJoin::Round)?;
                    pdf_page.objects_mut().add_path_object(obj)?;
                }
                pdf_page.regenerate_content()?;
                Ok(())
            }
        }
    }

    /// Emprise du dernier objet image de la page, en espace affiché (tests et diagnostics).
    pub fn last_image_bounds(&self, page: u16) -> CoreResult<Option<Rect>> {
        let _guard = engine::guard();
        let pdf_page = self
            .inner()
            .pages()
            .get(self.checked_index_unlocked(page)?)?;
        let frame = frame_of(&pdf_page);
        let mut last = None;
        for obj in pdf_page.objects().iter() {
            if obj.as_image_object().is_some()
                && let Ok(q) = obj.bounds()
            {
                last = Some(frame.from_user_space(Rect {
                    x: q.left().value,
                    y: q.bottom().value,
                    w: q.width().value,
                    h: q.height().value,
                }));
            }
        }
        Ok(last)
    }
}
