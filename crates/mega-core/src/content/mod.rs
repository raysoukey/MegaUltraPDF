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

use std::collections::BTreeSet;

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
/// lecteur PDF possède (jamais besoin d'embarquer une police), plus la police Unicode
/// incorporée pour les écritures que WinAnsi ne couvre pas (Lot 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontFamily {
    /// Sans empattement (Helvetica).
    Helvetica,
    /// Avec empattement (Times).
    Times,
    /// À chasse fixe (Courier).
    Courier,
    /// Police Unicode incorporée (DejaVu Sans, Lot 10) : grec, cyrillique, latin étendu.
    /// Choisie d'office pour un texte hors WinAnsi, quelle que soit la famille demandée.
    Unicode,
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

/// Octet WinAnsi (cp1252) de `c`, l'encodage des polices Standard 14 non embarquées :
/// ASCII imprimable et Latin-1 tels quels, plus les signes typographiques de la plage
/// 0x80-0x9F (€, guillemets courbes, tirets, …, œ, Œ, ™). `None` si le caractère n'y est pas.
pub(crate) fn win_ansi_byte(c: char) -> Option<u8> {
    let special = match c {
        '€' => 0x80,
        '‚' => 0x82,
        'ƒ' => 0x83,
        '„' => 0x84,
        '…' => 0x85,
        '†' => 0x86,
        '‡' => 0x87,
        'ˆ' => 0x88,
        '‰' => 0x89,
        'Š' => 0x8A,
        '‹' => 0x8B,
        'Œ' => 0x8C,
        'Ž' => 0x8E,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        '˜' => 0x98,
        '™' => 0x99,
        'š' => 0x9A,
        '›' => 0x9B,
        'œ' => 0x9C,
        'ž' => 0x9E,
        'Ÿ' => 0x9F,
        ' '..='~' | '\u{A0}'..='\u{FF}' => return u8::try_from(u32::from(c)).ok(),
        _ => return None,
    };
    Some(special)
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

/// Refuse de modifier une page dont PDFium **régénérerait** le flux en perdant du contenu.
///
/// Toute édition d'un objet existant (suppression, déplacement, texte, caviardage) fait
/// réécrire par PDFium le flux entier de la page à partir des objets analysés ; ce
/// générateur ne sait pas réémettre le texte en police **Type 3**, les images **en ligne**
/// (`BI … EI`) ni les ombrages (`sh`), qui disparaîtraient en silence. On inspecte la page
/// avec `lopdf` avant d'y toucher ; l'insertion (nouveau flux) n'est pas concernée.
pub(crate) fn check_regeneration_safe(bytes: &[u8], page: u16) -> CoreResult<()> {
    let refuse = |what: &str| {
        CoreError::InvalidOperation(format!(
            "cette page contient {what} : PDFium le perdrait en régénérant son contenu ; \
             modification des objets refusée (l'insertion reste possible)"
        ))
    };
    let Ok(doc) = lopdf::Document::load_mem(bytes) else {
        // Illisible par lopdf : on ne peut rien garantir.
        return Err(refuse("un contenu que l'on ne sait pas analyser"));
    };
    let Some(page_id) = doc.get_pages().get(&(u32::from(page) + 1)).copied() else {
        return Err(refuse("un contenu que l'on ne sait pas analyser"));
    };
    if let Ok(fonts) = doc.get_page_fonts(page_id)
        && fonts.values().any(|f| {
            f.get(b"Subtype")
                .ok()
                .and_then(|s| s.as_name().ok())
                .is_some_and(|n| n == b"Type3")
        })
    {
        return Err(refuse("du texte en police Type 3"));
    }
    let content = doc.get_page_content(page_id);
    let Ok(ops) = lopdf::content::Content::decode(&content) else {
        return Err(refuse("un contenu que l'on ne sait pas analyser"));
    };
    for op in &ops.operations {
        match op.operator.as_str() {
            "BI" | "ID" | "EI" => return Err(refuse("une image en ligne")),
            "sh" => return Err(refuse("un ombrage")),
            _ => {}
        }
    }
    Ok(())
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
        self.mark_changed();
        self.check_page_editable(page)?;
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
        self.mark_changed();
        if !(dx.is_finite() && dy.is_finite()) {
            return Err(invalid("déplacement invalide"));
        }
        self.check_page_editable(page)?;
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

    /// Redimensionne l'objet `index` pour qu'il occupe `rect` (points, espace **affiché**) :
    /// mise à l'échelle puis translation de sa matrice, en espace utilisateur (pages tournées
    /// comprises). Refuse un rectangle vide ou non fini, et un objet sans étendue.
    pub fn resize_object(&mut self, page: u16, index: u16, rect: Rect) -> CoreResult<()> {
        self.mark_changed();
        let finite = [rect.x, rect.y, rect.w, rect.h]
            .iter()
            .all(|v| v.is_finite());
        if !(finite && rect.w >= 1.0 && rect.h >= 1.0) {
            return Err(invalid("taille invalide (au moins 1 point de côté)"));
        }
        self.check_page_editable(page)?;
        let _guard = engine::guard();
        let mut pdf_page = self
            .inner()
            .pages()
            .get(self.checked_index_unlocked(page)?)?;
        check_index(&pdf_page, index)?;
        let target = frame_of(&pdf_page).to_user_space(rect);
        {
            let mut obj = pdf_page.objects().get(usize::from(index))?;
            let q = obj.bounds()?;
            let (bx, by, bw, bh) = (
                q.left().value,
                q.bottom().value,
                q.width().value,
                q.height().value,
            );
            if !(bw > 0.0 && bh > 0.0) {
                return Err(invalid("cet objet n'a pas d'étendue"));
            }
            // x' = sx·x + (cible.x − sx·bx) : le coin bas-gauche de l'emprise va sur celui de
            // la cible, et la largeur devient celle de la cible (idem en hauteur).
            let (sx, sy) = (target.w / bw, target.h / bh);
            obj.transform(sx, 0.0, 0.0, sy, target.x - sx * bx, target.y - sy * by)?;
        }
        pdf_page.regenerate_content()?;
        Ok(())
    }

    /// Remplace le texte de l'objet texte `index` (même police, même position : une retouche
    /// « au mieux », pas une recomposition). Refuse un texte vide ou trop long, et un objet
    /// qui n'est pas du texte. Dans une police **incorporée** (souvent un sous-ensemble
    /// `ABCDEF+Nom` qui ne contient que les glyphes utilisés), refuse aussi un caractère qui
    /// ne figure pas déjà dans le texte de l'objet : il s'afficherait mal (B22, Lot 9).
    pub fn set_object_text(&mut self, page: u16, index: u16, text: &str) -> CoreResult<()> {
        self.mark_changed();
        if text.trim().is_empty() {
            return Err(invalid("le texte ne peut pas être vide"));
        }
        if text.chars().count() > MAX_OBJECT_TEXT_CHARS {
            return Err(invalid("texte trop long (4096 caractères au plus)"));
        }
        self.check_page_editable(page)?;
        let _guard = engine::guard();
        let page_index = self.checked_index_unlocked(page)?;
        // Un texte hors WinAnsi demande la police Unicode (chargée avant d'emprunter la page).
        let unicode = if crate::fonts::needs_unicode(text) {
            crate::fonts::check_drawable(text)?;
            Some(self.unicode_font_unlocked()?)
        } else {
            None
        };
        let doc = self.inner();
        let mut pdf_page = doc.pages().get(page_index)?;
        check_index(&pdf_page, index)?;
        // Une police standard (non incorporée) n'a pas ces lettres : l'objet est remplacé par
        // un objet neuf en police Unicode, même matrice, même couleur, même place dans le flux.
        let mut replacement = None;
        {
            let mut obj = pdf_page.objects().get(usize::from(index))?;
            let t = obj
                .as_text_object_mut()
                .ok_or_else(|| invalid("cet objet n'est pas un texte"))?;
            // [Apprentissage] `font` emprunte `t` : le bloc rend l'emprunt avant `set_text`,
            // qui demande `t` en écriture (un emprunt partagé et un exclusif ne coexistent pas).
            let restricted = {
                let font = t.font();
                let name = font.name();
                let bytes = name.as_bytes();
                let subset =
                    bytes.get(6) == Some(&b'+') && bytes.iter().take(6).all(u8::is_ascii_uppercase);
                (subset || font.is_embedded().unwrap_or(false)).then_some(name)
            };
            if let Some(name) = &restricted {
                let known: BTreeSet<char> = t.text().chars().collect();
                // L'espace est admise : un sous-ensemble la contient presque toujours.
                if let Some(c) = text.chars().find(|c| *c != ' ' && !known.contains(c)) {
                    return Err(invalid(&format!(
                        "la police « {name} » n'est incorporée qu'en partie : le caractère « {c} » \
                         n'y figure peut-être pas (seuls les caractères déjà présents sont permis)"
                    )));
                }
            }
            match unicode {
                Some(font) if restricted.is_none() => {
                    replacement =
                        Some((font, t.matrix()?, t.unscaled_font_size(), t.fill_color()?));
                }
                _ => {
                    t.set_text(text)?;
                }
            }
        }
        if let Some((font, matrix, size, color)) = replacement {
            let position = usize::from(index);
            pdf_page.objects_mut().remove_object_at_index(position)?;
            let mut t = PdfPageTextObject::new(doc, text, font, size)?;
            t.set_fill_color(color)?;
            // Un objet neuf a la matrice identité : la composer revient à poser l'ancienne.
            t.apply_matrix(matrix)?;
            pdf_page
                .objects_mut()
                .insert_object_at_index(position, PdfPageObject::from(t))?;
        }
        pdf_page.regenerate_content()?;
        Ok(())
    }
}

impl Document {
    /// Insère `text` (une ligne par `\n`, interligne 1,2 × taille) dont le coin haut-gauche
    /// est `at` (points, espace **affiché**). Un texte hors WinAnsi est écrit avec la police
    /// Unicode incorporée. Refuse un texte vide ou trop long, un caractère qu'aucune de nos
    /// polices ne possède (CJK), une taille hors bornes et un point hors page.
    pub fn insert_text(
        &mut self,
        page: u16,
        at: [f32; 2],
        text: &str,
        style: &TextStyle,
    ) -> CoreResult<()> {
        self.mark_changed();
        if text.trim().is_empty() {
            return Err(invalid("le texte ne peut pas être vide"));
        }
        if text.chars().count() > MAX_OBJECT_TEXT_CHARS {
            return Err(invalid("texte trop long (4096 caractères au plus)"));
        }
        crate::fonts::check_drawable(text)?;
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
        // Hors WinAnsi, les polices Standard 14 dessineraient des « ÿ » : la police Unicode
        // remplace la famille demandée (sans gras ni italique : DejaVu Sans seule est
        // incorporée).
        let family = if crate::fonts::needs_unicode(text) {
            FontFamily::Unicode
        } else {
            style.family
        };
        let font = if family == FontFamily::Unicode {
            self.unicode_font_unlocked()?
        } else {
            let fonts = self.inner_mut().fonts_mut();
            match (family, style.bold, style.italic) {
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
                (FontFamily::Unicode, _, _) => return Err(invalid("police Unicode attendue")),
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
        self.mark_changed();
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

impl Document {
    /// Vérifie que la page `page` existe et supporte la régénération de son contenu
    /// (voir [`check_regeneration_safe`]). Prend et rend le jeton : à appeler **avant** la
    /// phase PDFium.
    fn check_page_editable(&self, page: u16) -> CoreResult<()> {
        {
            let _guard = engine::guard();
            self.checked_index_unlocked(page)?;
        }
        self.check_not_encrypted()?;
        let bytes = self.to_bytes()?;
        check_regeneration_safe(&bytes, page)
    }

    /// Refuse l'édition du contenu d'un PDF chiffré : les objets retirés resteraient en
    /// orphelins dans le fichier (la passe `lopdf` ne peut pas l'élaguer) et l'analyse de
    /// la page n'est pas possible. Prend et rend le jeton.
    pub(crate) fn check_not_encrypted(&self) -> CoreResult<()> {
        let _guard = engine::guard();
        let unprotected = self
            .inner()
            .permissions()
            .security_handler_revision()
            .map(|r| r == pdfium_render::prelude::PdfSecurityHandlerRevision::Unprotected)
            .unwrap_or(false);
        if unprotected {
            Ok(())
        } else {
            Err(invalid(
                "édition du contenu d'un PDF chiffré : non prise en charge (le contenu retiré \
                 resterait dans le fichier)",
            ))
        }
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
