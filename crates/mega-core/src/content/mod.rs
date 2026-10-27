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

pub mod image;
pub mod redact;

use pdfium_render::prelude::{
    PdfPageContentRegenerationStrategy, PdfPageObject, PdfPageObjectCommon, PdfPageObjectType,
    PdfPageObjectsCommon, PdfPageTextObject, PdfPoints,
};
use serde::{Deserialize, Serialize};

use crate::annotations::Color;
use crate::document::Document;
use crate::engine;
use crate::error::{CoreError, CoreResult};
use crate::geometry::{PageFrame, Rect, frame_of};

/// Nombre maximal d'objets lus par page (un PDF est une donnée non fiable).
pub const MAX_OBJECTS_PER_PAGE: usize = 5000;
/// Longueur maximale (en caractères) du texte d'un objet, lu ou écrit.
pub const MAX_OBJECT_TEXT_CHARS: usize = 4096;
/// Taille de police minimale d'un texte inséré (points).
pub const MIN_FONT_SIZE: f32 = 4.0;
/// Taille de police maximale d'un texte inséré (points).
pub const MAX_FONT_SIZE: f32 = 400.0;
/// Interligne d'un texte inséré, en multiple de la taille de police.
const LINE_HEIGHT: f32 = 1.2;
/// Distance du haut du bloc à la première ligne de base, en multiple de la taille.
const FIRST_BASELINE: f32 = 0.8;

/// Famille de police d'un texte inséré : les trois familles **Standard 14** que tout
/// lecteur PDF possède, donc jamais besoin d'embarquer une police.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontFamily {
    /// Sans empattement (Helvetica).
    Helvetica,
    /// Avec empattement (Times).
    Times,
    /// À chasse fixe (Courier).
    Courier,
}

/// Style d'un texte inséré dans le contenu.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    /// Famille.
    pub family: FontFamily,
    /// Gras.
    pub bold: bool,
    /// Italique (oblique pour Helvetica et Courier).
    pub italic: bool,
    /// Taille en points ([`MIN_FONT_SIZE`] à [`MAX_FONT_SIZE`]).
    pub size: f32,
    /// Couleur du texte.
    pub color: Color,
}

/// Vrai si `c` existe dans l'encodage WinAnsi (cp1252), celui des polices Standard 14 non
/// embarquées : ASCII imprimable, Latin-1, et les quelques signes typographiques de la plage
/// 0x80-0x9F (€, guillemets courbes, tirets, …, œ, Œ, ™).
fn is_win_ansi(c: char) -> bool {
    matches!(
        c,
        ' '..='~'
            | '\u{A0}'..='\u{FF}'
            | '€'
            | '‚'
            | 'ƒ'
            | '„'
            | '…'
            | '†'
            | '‡'
            | 'ˆ'
            | '‰'
            | 'Š'
            | '‹'
            | 'Œ'
            | 'Ž'
            | '‘'
            | '’'
            | '“'
            | '”'
            | '•'
            | '–'
            | '—'
            | '˜'
            | '™'
            | 'š'
            | '›'
            | 'œ'
            | 'ž'
            | 'Ÿ'
    )
}

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
    /// Nom de la police (`/BaseFont`), pour un objet texte.
    pub font: Option<String>,
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
        font: text_object.map(|t| t.font().name().chars().take(128).collect()),
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

impl Document {
    /// Insère `text` (une ligne par `\n`, interligne 1,2 × taille) dont le coin haut-gauche
    /// est `at` (points, espace **affiché**). Refuse un texte vide ou trop long, un caractère
    /// hors WinAnsi (les polices Standard 14 ne savent pas l'afficher), une taille hors
    /// bornes et un point hors page.
    pub fn insert_text(
        &mut self,
        page: u16,
        at: [f32; 2],
        text: &str,
        style: &TextStyle,
    ) -> CoreResult<()> {
        if text.trim().is_empty() {
            return Err(invalid("le texte ne peut pas être vide"));
        }
        if text.chars().count() > MAX_OBJECT_TEXT_CHARS {
            return Err(invalid("texte trop long (4096 caractères au plus)"));
        }
        if let Some(c) = text
            .chars()
            .find(|c| !is_win_ansi(*c) && *c != '\n' && *c != '\r')
        {
            return Err(CoreError::InvalidOperation(format!(
                "caractère non pris en charge : « {c} » (polices standard, encodage WinAnsi)"
            )));
        }
        if !(style.size.is_finite() && (MIN_FONT_SIZE..=MAX_FONT_SIZE).contains(&style.size)) {
            return Err(invalid(
                "la taille de police doit être comprise entre 4 et 400 points",
            ));
        }
        if !(at[0].is_finite() && at[1].is_finite()) {
            return Err(invalid("position invalide"));
        }
        let _guard = engine::guard();
        let page_index = self.checked_index_unlocked(page)?;
        // La police est enregistrée sur le document (emprunt mutable, terminé avant la suite).
        let font = {
            let fonts = self.inner_mut().fonts_mut();
            match (style.family, style.bold, style.italic) {
                (FontFamily::Helvetica, false, false) => fonts.helvetica(),
                (FontFamily::Helvetica, true, false) => fonts.helvetica_bold(),
                (FontFamily::Helvetica, false, true) => fonts.helvetica_oblique(),
                (FontFamily::Helvetica, true, true) => fonts.helvetica_bold_oblique(),
                (FontFamily::Times, false, false) => fonts.times_roman(),
                (FontFamily::Times, true, false) => fonts.times_bold(),
                (FontFamily::Times, false, true) => fonts.times_italic(),
                (FontFamily::Times, true, true) => fonts.times_bold_italic(),
                (FontFamily::Courier, false, false) => fonts.courier(),
                (FontFamily::Courier, true, false) => fonts.courier_bold(),
                (FontFamily::Courier, false, true) => fonts.courier_oblique(),
                (FontFamily::Courier, true, true) => fonts.courier_bold_oblique(),
            }
        };
        let doc = self.inner();
        let mut pdf_page = doc.pages().get(page_index)?;
        let frame = frame_of(&pdf_page);
        point_in_page(&frame, at)?;
        let matrix = (!frame.is_identity()).then(|| frame.matrix_to_user_space());
        let color = style.color.with_opacity(1.0);
        // Un seul flux de contenu pour tout le bloc (sinon un flux par ligne).
        pdf_page.set_content_regeneration_strategy(PdfPageContentRegenerationStrategy::Manual);
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            // Un objet texte neuf a son origine sur la ligne de base, au début du texte :
            // `translate` le pose exactement là où la ligne doit commencer.
            let baseline =
                at[1] - style.size * FIRST_BASELINE - (i as f32) * style.size * LINE_HEIGHT;
            let mut t = PdfPageTextObject::new(doc, line, font, PdfPoints::new(style.size))?;
            t.set_fill_color(color)?;
            t.translate(PdfPoints::new(at[0]), PdfPoints::new(baseline))?;
            if let Some(m) = matrix {
                t.apply_matrix(pdfium_render::prelude::PdfMatrix::new(
                    m[0], m[1], m[2], m[3], m[4], m[5],
                ))?;
            }
            pdf_page.objects_mut().add_text_object(t)?;
        }
        pdf_page.regenerate_content()?;
        Ok(())
    }

    /// Insère l'image du fichier `path` (PNG ou JPEG, bornes de [`image`]) ajustée et
    /// centrée dans `rect` (points, espace **affiché**).
    pub fn insert_image(&mut self, page: u16, rect: Rect, path: &str) -> CoreResult<()> {
        if !(rect.w > 0.0 && rect.h > 0.0 && rect.w.is_finite() && rect.h.is_finite()) {
            return Err(invalid("la zone de l'image est vide"));
        }
        let loaded = image::load_bounded(path)?;
        let placed = image::fit(loaded.width() as f32, loaded.height() as f32, &rect);
        let _guard = engine::guard();
        let mut pdf_page = self
            .inner()
            .pages()
            .get(self.checked_index_unlocked(page)?)?;
        let frame = frame_of(&pdf_page);
        rect_in_page(&frame, &rect)?;
        image::place(&mut pdf_page, &frame, &loaded, &placed)
    }
}

/// Un point doit tomber dans la page affichée (1 pt de tolérance).
fn point_in_page(frame: &PageFrame, p: [f32; 2]) -> CoreResult<()> {
    let (w, h) = frame.displayed_size();
    if p[0] < -1.0 || p[1] < -1.0 || p[0] > w + 1.0 || p[1] > h + 1.0 {
        return Err(invalid("le point sort de la page"));
    }
    Ok(())
}

/// Un rectangle doit tenir dans la page affichée (1 pt de tolérance).
pub(crate) fn rect_in_page(frame: &PageFrame, r: &Rect) -> CoreResult<()> {
    let (w, h) = frame.displayed_size();
    if r.x < -1.0 || r.y < -1.0 || r.x + r.w > w + 1.0 || r.y + r.h > h + 1.0 {
        return Err(invalid("la zone sort de la page"));
    }
    Ok(())
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
