// SPDX-License-Identifier: GPL-3.0-or-later
//! Caviardage **réel** : ce qui tombe sous le rectangle est retiré du fichier, puis un cache
//! noir est dessiné. Un rectangle noir seul (comme le font beaucoup d'outils) laisse le texte
//! dans le flux de contenu, où n'importe quel lecteur peut le copier.
//!
//! Granularité : l'**objet** de page. Un objet texte est une suite de glyphes posée d'un
//! bloc (souvent un mot ou une ligne) ; on le retire entier dès qu'il touche la zone, quitte
//! à en enlever un peu plus que demandé — en matière de caviardage, c'est le bon côté où se
//! tromper. Les images qui touchent la zone sont retirées entières ; les chemins et les
//! formulaires XObject sont seulement recouverts (limite documentée, ADR-0010).
//!
//! \[Apprentissage\] **Parcourir à l'envers pour supprimer** : retirer l'objet 2 décale
//! l'objet 3 en position 2. En partant de la fin, les positions encore à visiter ne bougent
//! pas.

use pdfium_render::prelude::{
    PdfColor, PdfPageAnnotationCommon, PdfPageContentRegenerationStrategy, PdfPageObjectCommon,
    PdfPageObjectType, PdfPageObjectsCommon, PdfPagePathObject,
};
use serde::{Deserialize, Serialize};

use crate::annotations::to_pdf_rect;
use crate::document::Document;
use crate::engine;
use crate::error::{CoreError, CoreResult};
use crate::geometry::{Rect, frame_of};

/// Ce qu'un caviardage a retiré.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RedactReport {
    /// Objets texte supprimés.
    pub texts: u16,
    /// Objets image supprimés.
    pub images: u16,
    /// Annotations supprimées.
    pub annotations: u16,
}

/// Vrai si les deux rectangles se chevauchent sur une surface non nulle.
fn intersects(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y
}

fn rect_of(q: pdfium_render::prelude::PdfQuadPoints) -> Rect {
    Rect {
        x: q.left().value,
        y: q.bottom().value,
        w: q.width().value,
        h: q.height().value,
    }
}

impl Document {
    /// Caviarde `rect` (points, espace **affiché**) sur la page `page` : retire les objets
    /// texte et image et les annotations qui touchent la zone, puis la recouvre d'un
    /// rectangle noir. Retourne ce qui a été retiré.
    pub fn redact(&mut self, page: u16, rect: Rect) -> CoreResult<RedactReport> {
        if !(rect.w > 0.0 && rect.h > 0.0 && rect.w.is_finite() && rect.h.is_finite()) {
            return Err(CoreError::InvalidOperation(
                "la zone à caviarder est vide".to_string(),
            ));
        }
        let _guard = engine::guard();
        let page_index = self.checked_index_unlocked(page)?;
        let doc = self.inner();
        let mut pdf_page = doc.pages().get(page_index)?;
        let frame = frame_of(&pdf_page);
        super::rect_in_page(&frame, &rect)?;
        let zone = frame.to_user_space(rect);
        let mut report = RedactReport::default();
        // Un seul flux régénéré à la fin, pas un par suppression.
        pdf_page.set_content_regeneration_strategy(PdfPageContentRegenerationStrategy::Manual);
        let count = pdf_page.objects().len();
        for i in (0..count).rev() {
            let obj = pdf_page.objects().get(i)?;
            let kind = obj.object_type();
            if !matches!(kind, PdfPageObjectType::Text | PdfPageObjectType::Image) {
                continue;
            }
            let Ok(bounds) = obj.bounds() else {
                continue;
            };
            if !intersects(&rect_of(bounds), &zone) {
                continue;
            }
            drop(obj);
            pdf_page.objects_mut().remove_object_at_index(i)?;
            match kind {
                PdfPageObjectType::Text => report.texts = report.texts.saturating_add(1),
                _ => report.images = report.images.saturating_add(1),
            }
        }
        let annots = pdf_page.annotations_mut();
        for i in (0..annots.len()).rev() {
            let ann = annots.get(i)?;
            let Ok(bounds) = ann.bounds() else {
                continue;
            };
            if intersects(&Rect::from_pdf(bounds), &zone) {
                annots.delete_annotation(ann)?;
                report.annotations = report.annotations.saturating_add(1);
            }
        }
        let black = PdfColor::new(0, 0, 0, 255);
        let cover = PdfPagePathObject::new_rect(doc, to_pdf_rect(&zone), None, None, Some(black))?;
        pdf_page.objects_mut().add_path_object(cover)?;
        pdf_page.regenerate_content()?;
        Ok(report)
    }
}
