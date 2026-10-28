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

/// Ce qu'un caviardage a retiré, pour en rendre compte à l'utilisateur (et lui signaler ce
/// qui n'a été que recouvert).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RedactReport {
    /// Objets texte supprimés.
    pub texts: u16,
    /// Images retirées entières (entièrement couvertes, ou impossibles à réécrire).
    pub images_removed: u16,
    /// Images remplacées par une copie dont les pixels sous la zone sont noirs (Lot 10).
    pub images_redacted: u16,
    /// Tracés vectoriels supprimés (contenus dans la zone).
    pub paths_removed: u16,
    /// Tracés qui traversent la zone : conservés sous le cache noir (signalés).
    pub paths_kept: u16,
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
    ///
    /// Deux refus protègent la promesse « le contenu disparaît du fichier » : un document
    /// **chiffré** (les objets retirés resteraient en orphelins, la passe `lopdf` ne pouvant
    /// pas l'élaguer) et du **texte qui resterait sous le cache** (dessiné à travers un
    /// formulaire XObject, hors de portée du retrait par objets) : le document est alors
    /// remis dans son état d'avant.
    pub fn redact(&mut self, page: u16, rect: Rect) -> CoreResult<RedactReport> {
        self.mark_changed();
        if !(rect.w > 0.0 && rect.h > 0.0 && rect.w.is_finite() && rect.h.is_finite()) {
            return Err(CoreError::InvalidOperation(
                "la zone à caviarder est vide".to_string(),
            ));
        }
        self.checked_index(page)?;
        self.check_not_encrypted()?;
        // Instantané pour revenir en arrière si la phase PDFium échoue à mi-chemin ou si du
        // texte reste sous le cache ; il sert aussi à vérifier que la page supporte la
        // régénération de son flux (B3).
        let snapshot = self.to_bytes()?;
        super::check_regeneration_safe(&snapshot, page)?;
        let report = match self
            .redact_objects(page, rect)
            .and_then(|report| self.redact_images(page, rect, report))
        {
            Ok(report) => report,
            Err(e) => {
                self.reload_from_bytes(snapshot)?;
                return Err(e);
            }
        };
        let remaining = self
            .text_boxes(page)?
            .into_iter()
            .filter(|b| {
                let (cx, cy) = (b.rect.x + b.rect.w / 2.0, b.rect.y + b.rect.h / 2.0);
                cx >= rect.x && cx <= rect.x + rect.w && cy >= rect.y && cy <= rect.y + rect.h
            })
            .count();
        if remaining > 0 {
            self.reload_from_bytes(snapshot)?;
            return Err(CoreError::InvalidOperation(format!(
                "du texte reste sous la zone ({remaining} caractère(s) dessinés via un groupe \
                 XObject) : le caviardage par objets ne peut pas le retirer"
            )));
        }
        Ok(report)
    }

    /// Phase `lopdf` : les images qui touchent la zone sont noircies dessous ou retirées
    /// (module [`super::raster`]), puis le document est rechargé.
    fn redact_images(
        &mut self,
        page: u16,
        rect: Rect,
        mut report: RedactReport,
    ) -> CoreResult<RedactReport> {
        let zone = self.page_frame(page)?.to_user_space(rect);
        let mut doc = lopdf::Document::load_mem(&self.to_bytes()?)
            .map_err(|e| CoreError::InvalidOperation(format!("caviardage des images : {e}")))?;
        let page_id = *doc
            .get_pages()
            .get(&(u32::from(page) + 1))
            .ok_or_else(|| CoreError::InvalidOperation("page introuvable".to_string()))?;
        let raster = super::raster::redact_images(&mut doc, page_id, &zone)?;
        report.images_removed = raster.removed;
        report.images_redacted = raster.redacted;
        if raster != super::raster::RasterReport::default() {
            let mut out = Vec::new();
            doc.save_to(&mut out)?;
            self.reload_from_bytes(out)?;
        }
        Ok(report)
    }

    /// Vérifie l'index de page (jeton pris le temps de la vérification).
    fn checked_index(&self, page: u16) -> CoreResult<()> {
        let _guard = engine::guard();
        self.checked_index_unlocked(page).map(|_| ())
    }

    /// Phase PDFium du caviardage : suppressions, cache noir, régénération du contenu.
    fn redact_objects(&mut self, page: u16, rect: Rect) -> CoreResult<RedactReport> {
        let _guard = engine::guard();
        let page_index = self.checked_index_unlocked(page)?;
        let doc = self.inner();
        let mut pdf_page = doc.pages().get(page_index)?;
        let frame = frame_of(&pdf_page);
        super::rect_in_page(&frame, &rect)?;
        let zone = frame.to_user_space(rect);
        let mut report = RedactReport::default();
        // Un widget retiré de `/Annots` laisserait sa valeur dans `/AcroForm /Fields` : on
        // refuse avant de toucher à quoi que ce soit (aplatir le formulaire d'abord).
        for ann in pdf_page.annotations().iter() {
            if ann.annotation_type() == pdfium_render::prelude::PdfPageAnnotationType::Widget
                && ann
                    .bounds()
                    .map(|b| intersects(&Rect::from_pdf(b), &zone))
                    .unwrap_or(false)
            {
                return Err(CoreError::InvalidOperation(
                    "la zone touche un champ de formulaire : sa valeur resterait dans le \
                     fichier ; aplatissez le formulaire avant de caviarder"
                        .to_string(),
                ));
            }
        }
        // Un seul flux régénéré à la fin, pas un par suppression.
        pdf_page.set_content_regeneration_strategy(PdfPageContentRegenerationStrategy::Manual);
        let count = pdf_page.objects().len();
        for i in (0..count).rev() {
            let obj = pdf_page.objects().get(i)?;
            let kind = obj.object_type();
            // Les images sont traitées ensuite par `lopdf` (module `raster`) : pixels noircis.
            if kind != PdfPageObjectType::Text {
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
            report.texts = report.texts.saturating_add(1);
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
