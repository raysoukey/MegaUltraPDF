// SPDX-License-Identifier: GPL-3.0-or-later
//! Signature visuelle : une image ou des traits ajoutés au **contenu** de la page.
//!
//! Ce n'est pas un champ `/Sig` (signature cryptographique, Lot 7) : c'est un dessin
//! permanent, visible dans tous les lecteurs, que personne ne pourra « décocher ».
//!
//! \[Apprentissage\] **Entrée non fiable** : le fichier image vient de l'utilisateur, mais
//! un PNG peut être piégé (dimensions énormes, données corrompues). On borne la taille du
//! fichier et des pixels **avant** de décoder, et le décodage faillible (`image::load_from_memory`)
//! est traduit en erreur métier plutôt qu'en panique.

use pdfium_render::prelude::{
    PdfColor, PdfPageObjectCommon, PdfPageObjectLineCap, PdfPageObjectLineJoin,
    PdfPageObjectsCommon, PdfPagePathObject, PdfPoints,
};
use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::engine;
use crate::error::{CoreError, CoreResult};
use crate::geometry::Rect;

/// Taille maximale d'un fichier image de signature (octets).
pub const MAX_SIGNATURE_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
/// Dimension maximale (pixels) d'une image de signature.
pub const MAX_SIGNATURE_IMAGE_PX: u32 = 8192;
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

/// Ajuste `(w, h)` dans `rect` en gardant les proportions, centré : `(x, y, w, h)`.
pub(crate) fn fit(image_w: f32, image_h: f32, rect: &Rect) -> Rect {
    let scale = (rect.w / image_w).min(rect.h / image_h);
    let w = image_w * scale;
    let h = image_h * scale;
    Rect {
        x: rect.x + (rect.w - w) / 2.0,
        y: rect.y + (rect.h - h) / 2.0,
        w,
        h,
    }
}

impl Document {
    /// Appose une signature visuelle dans `rect` (points PDF) sur la page `page`.
    pub fn insert_signature(
        &mut self,
        page: u16,
        rect: Rect,
        source: &SignatureSource,
    ) -> CoreResult<()> {
        if !(rect.w > 0.0 && rect.h > 0.0 && rect.w.is_finite() && rect.h.is_finite()) {
            return Err(invalid("la zone de signature est vide".to_string()));
        }
        match source {
            SignatureSource::Image { path } => {
                let size = std::fs::metadata(path)?.len();
                if size > MAX_SIGNATURE_IMAGE_BYTES {
                    return Err(invalid(format!(
                        "image trop volumineuse ({} Mo, maximum 20 Mo)",
                        size / (1024 * 1024)
                    )));
                }
                let bytes = std::fs::read(path)?;
                let image = image::load_from_memory(&bytes)
                    .map_err(|e| invalid(format!("image illisible : {e}")))?;
                if image.width() > MAX_SIGNATURE_IMAGE_PX || image.height() > MAX_SIGNATURE_IMAGE_PX
                {
                    return Err(invalid(format!(
                        "image trop grande ({}×{} px, maximum {MAX_SIGNATURE_IMAGE_PX})",
                        image.width(),
                        image.height()
                    )));
                }
                let placed = fit(image.width() as f32, image.height() as f32, &rect);
                let _guard = engine::guard();
                let mut pdf_page = self
                    .inner()
                    .pages()
                    .get(self.checked_index_unlocked(page)?)?;
                pdf_page.objects_mut().create_image_object(
                    PdfPoints::new(placed.x),
                    PdfPoints::new(placed.y),
                    &image,
                    Some(PdfPoints::new(placed.w)),
                    Some(PdfPoints::new(placed.h)),
                )?;
                pdf_page.regenerate_content()?;
                Ok(())
            }
            SignatureSource::Strokes { strokes } => {
                let valid: Vec<&Vec<[f32; 2]>> = strokes
                    .iter()
                    .filter(|s| {
                        s.len() >= 2 && s.iter().all(|p| p[0].is_finite() && p[1].is_finite())
                    })
                    .collect();
                if valid.is_empty() {
                    return Err(invalid(
                        "une signature dessinée exige au moins un trait".to_string(),
                    ));
                }
                let _guard = engine::guard();
                let page_index = self.checked_index_unlocked(page)?;
                let doc = self.inner();
                let mut pdf_page = doc.pages().get(page_index)?;
                let black = PdfColor::new(0, 0, 0, 255);
                for stroke in valid {
                    let mut obj = PdfPagePathObject::new(
                        doc,
                        PdfPoints::new(stroke[0][0]),
                        PdfPoints::new(stroke[0][1]),
                        Some(black),
                        Some(PdfPoints::new(STROKE_WIDTH)),
                        None,
                    )?;
                    for p in &stroke[1..] {
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

    /// Emprise du dernier objet image de la page (tests et diagnostics).
    pub fn last_image_bounds(&self, page: u16) -> CoreResult<Option<Rect>> {
        let _guard = engine::guard();
        let pdf_page = self
            .inner()
            .pages()
            .get(self.checked_index_unlocked(page)?)?;
        let mut last = None;
        for obj in pdf_page.objects().iter() {
            if obj.as_image_object().is_some()
                && let Ok(q) = obj.bounds()
            {
                last = Some(Rect {
                    x: q.left().value,
                    y: q.bottom().value,
                    w: q.width().value,
                    h: q.height().value,
                });
            }
        }
        Ok(last)
    }
}
