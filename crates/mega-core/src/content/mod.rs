// SPDX-License-Identifier: GPL-3.0-or-later
//! Objets de page (texte, images, chemins…) : lecture, suppression, déplacement et
//! remplacement du texte d'un objet (ADR-0010).
//!
//! Un objet de page est une instruction du flux de contenu : « dessine ce texte ici »,
//! « pose cette image là ». PDFium les expose un par un, dans l'ordre du flux ; l'index est
//! donc **positionnel** et peut changer après une suppression (comme pour les annotations).
//!
//! \[Apprentissage\] **Index positionnel et invalidation** : après `delete_object(0)`,
//! l'ancien objet 1 devient l'objet 0. L'interface relit donc la liste après chaque édition
//! plutôt que de garder des index en mémoire — la même discipline que pour `/Annots`.

use pdfium_render::prelude::{
    PdfPageObject, PdfPageObjectCommon, PdfPageObjectType, PdfPageObjectsCommon, PdfPageTextObject,
    PdfPoints,
};
use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::engine;
use crate::error::{CoreError, CoreResult};
use crate::geometry::{Rect, frame_of};

/// Nombre maximal d'objets lus par page (un PDF est une donnée non fiable).
pub const MAX_OBJECTS_PER_PAGE: usize = 5000;
/// Longueur maximale (en caractères) du texte d'un objet, lu ou écrit.
pub const MAX_OBJECT_TEXT_CHARS: usize = 4096;

/// Nature d'un objet de page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    /// Texte.
    Text,
    /// Image.
    Image,
    /// Tracé vectoriel.
    Path,
    /// Formulaire XObject (groupe d'objets réutilisable, sans rapport avec les champs).
    Form,
    /// Dégradé.
    Shading,
    /// Tout le reste.
    Other,
}

impl ObjectKind {
    /// Nom affichable en français (libellés d'historique et CLI).
    pub fn label_fr(self) -> &'static str {
        match self {
            ObjectKind::Text => "Texte",
            ObjectKind::Image => "Image",
            ObjectKind::Path => "Tracé",
            ObjectKind::Form => "Groupe",
            ObjectKind::Shading => "Dégradé",
            ObjectKind::Other => "Objet",
        }
    }

    fn from_pdfium(t: PdfPageObjectType) -> ObjectKind {
        match t {
            PdfPageObjectType::Text => ObjectKind::Text,
            PdfPageObjectType::Image => ObjectKind::Image,
            PdfPageObjectType::Path => ObjectKind::Path,
            PdfPageObjectType::XObjectForm => ObjectKind::Form,
            PdfPageObjectType::Shading => ObjectKind::Shading,
            PdfPageObjectType::Unsupported => ObjectKind::Other,
        }
    }
}

/// Un objet de page tel que l'interface le présente (coordonnées **affichées**).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageObject {
    /// Position dans le flux de contenu (identifiant pour supprimer / déplacer).
    pub index: u16,
    /// Nature.
    pub kind: ObjectKind,
    /// Emprise, en points PDF affichés.
    pub bounds: Rect,
    /// Le texte, pour un objet texte (au plus [`MAX_OBJECT_TEXT_CHARS`] caractères).
    pub text: Option<String>,
    /// Taille de police effective, pour un objet texte.
    pub font_size: Option<f32>,
}

fn invalid(msg: &str) -> CoreError {
    CoreError::InvalidOperation(msg.to_string())
}

/// Taille de police **effective** d'un objet texte : la taille nominale multipliée par
/// l'échelle verticale de sa matrice, lue comme la norme de la colonne (b, d).
///
/// `scaled_font_size()` de pdfium-render ne lit que le coefficient `d`, nul pour un texte
/// tourné d'un quart de tour (page `/Rotate` ou texte vertical) : on aurait une taille 0.
pub(crate) fn effective_font_size(t: &PdfPageTextObject<'_>) -> f32 {
    let scale = t
        .matrix()
        .map(|m| (m.b() * m.b() + m.d() * m.d()).sqrt())
        .unwrap_or(1.0);
    t.unscaled_font_size().value * scale
}

fn read_object(
    index: u16,
    obj: &PdfPageObject<'_>,
    frame: &crate::geometry::PageFrame,
) -> PageObject {
    let kind = ObjectKind::from_pdfium(obj.object_type());
    let bounds = obj
        .bounds()
        .map(|q| Rect {
            x: q.left().value,
            y: q.bottom().value,
            w: q.width().value,
            h: q.height().value,
        })
        .unwrap_or(Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        });
    let text_object = obj.as_text_object();
    PageObject {
        index,
        kind,
        bounds: frame.from_user_space(bounds),
        text: text_object.map(|t| t.text().chars().take(MAX_OBJECT_TEXT_CHARS).collect()),
        font_size: text_object.map(effective_font_size),
    }
}

impl Document {
    /// Les objets de la page `page`, dans l'ordre du flux de contenu (au plus
    /// [`MAX_OBJECTS_PER_PAGE`]).
    pub fn page_objects(&self, page: u16) -> CoreResult<Vec<PageObject>> {
        let _guard = engine::guard();
        let pdf_page = self
            .inner()
            .pages()
            .get(self.checked_index_unlocked(page)?)?;
        let frame = frame_of(&pdf_page);
        Ok(pdf_page
            .objects()
            .iter()
            .take(MAX_OBJECTS_PER_PAGE)
            .enumerate()
            .map(|(i, obj)| read_object(i as u16, &obj, &frame))
            .collect())
    }

    /// Supprime l'objet `index` de la page (les index suivants reculent d'un cran).
    pub fn delete_object(&mut self, page: u16, index: u16) -> CoreResult<()> {
        let _guard = engine::guard();
        let mut pdf_page = self
            .inner()
            .pages()
            .get(self.checked_index_unlocked(page)?)?;
        check_index(&pdf_page, index)?;
        pdf_page
            .objects_mut()
            .remove_object_at_index(usize::from(index))?;
        pdf_page.regenerate_content()?;
        Ok(())
    }

    /// Déplace l'objet `index` de `(dx, dy)` points, en axes **affichés** (sur une page
    /// tournée, « vers la droite à l'écran » reste vers la droite à l'écran).
    pub fn move_object(&mut self, page: u16, index: u16, dx: f32, dy: f32) -> CoreResult<()> {
        if !(dx.is_finite() && dy.is_finite()) {
            return Err(invalid("déplacement invalide"));
        }
        let _guard = engine::guard();
        let mut pdf_page = self
            .inner()
            .pages()
            .get(self.checked_index_unlocked(page)?)?;
        check_index(&pdf_page, index)?;
        let frame = frame_of(&pdf_page);
        let [ux, uy] = frame.vector_to_user_space([dx, dy]);
        // L'objet emprunte la page : le bloc rend l'emprunt avant la régénération.
        {
            let mut obj = pdf_page.objects().get(usize::from(index))?;
            obj.translate(PdfPoints::new(ux), PdfPoints::new(uy))?;
        }
        pdf_page.regenerate_content()?;
        Ok(())
    }

    /// Remplace le texte de l'objet texte `index` (même police, même position : une retouche
    /// « au mieux », pas une recomposition). Refuse un texte vide ou trop long, et un objet
    /// qui n'est pas du texte.
    pub fn set_object_text(&mut self, page: u16, index: u16, text: &str) -> CoreResult<()> {
        if text.trim().is_empty() {
            return Err(invalid("le texte ne peut pas être vide"));
        }
        if text.chars().count() > MAX_OBJECT_TEXT_CHARS {
            return Err(invalid("texte trop long (4096 caractères au plus)"));
        }
        let _guard = engine::guard();
        let mut pdf_page = self
            .inner()
            .pages()
            .get(self.checked_index_unlocked(page)?)?;
        check_index(&pdf_page, index)?;
        {
            let mut obj = pdf_page.objects().get(usize::from(index))?;
            let t = obj
                .as_text_object_mut()
                .ok_or_else(|| invalid("cet objet n'est pas un texte"))?;
            t.set_text(text)?;
        }
        pdf_page.regenerate_content()?;
        Ok(())
    }
}

/// Vérifie qu'un index d'objet existe sur la page, **jeton déjà pris par l'appelant**.
fn check_index(pdf_page: &pdfium_render::prelude::PdfPage<'_>, index: u16) -> CoreResult<()> {
    let count = pdf_page.objects().len();
    if usize::from(index) >= count {
        return Err(CoreError::InvalidOperation(format!(
            "objet {index} introuvable (la page en a {count})"
        )));
    }
    Ok(())
}
