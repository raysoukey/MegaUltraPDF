// SPDX-License-Identifier: GPL-3.0-or-later
//! Annotations : lecture du modèle, création avec apparence portable, suppression,
//! déplacement.
//!
//! \[Apprentissage\] **`enum` riche et `match` exhaustif** : PDFium distingue une trentaine
//! de sous-types d'annotations ; nous les ramenons à une poignée de « genres » utiles à
//! l'interface ([`AnnotationKind`]). Le `match` sur le sous-type est exhaustif : si une
//! future version de pdfium-render ajoute un sous-type, le compilateur nous le signalera.
//!
//! Choix de représentation (ADR-0008) : les formes (rectangle, ellipse, ligne, flèche) et
//! l'encre sont des annotations `Ink` dont l'apparence est dessinée par des objets chemin ;
//! le texte libre et les tampons sont des `Stamp` dessinés par des objets texte. Ce sont les
//! deux seuls sous-types auxquels pdfium-render permet d'ajouter une apparence sans `unsafe`,
//! et une apparence explicite est ce que **tous** les lecteurs affichent à l'identique.

mod appearance;

use chrono::Utc;
use pdfium_render::prelude::{
    PdfColor, PdfPageAnnotation, PdfPageAnnotationCommon, PdfPageAnnotationType,
    PdfPageObjectCommon, PdfPageObjectsCommon, PdfPoints, PdfQuadPoints, PdfRect,
};
use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::engine;
use crate::error::{CoreError, CoreResult};
use crate::geometry::Rect;

/// Nombre maximal d'annotations lues par page (un PDF est une donnée non fiable).
pub const MAX_ANNOTATIONS_PER_PAGE: usize = 2000;
/// Nombre maximal de quadrilatères lus par marquage de texte.
pub const MAX_QUADS_PER_ANNOTATION: usize = 512;
/// Longueur maximale (en caractères) des chaînes lues (`/Contents`, `/T`, dates).
pub const MAX_STRING_CHARS: usize = 4096;

/// Genre d'annotation, tel que l'interface le présente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationKind {
    /// Note (icône) avec un commentaire.
    Note,
    /// Surlignage de texte.
    Highlight,
    /// Soulignement de texte.
    Underline,
    /// Texte barré.
    StrikeOut,
    /// Soulignement ondulé.
    Squiggly,
    /// Rectangle.
    Rect,
    /// Ellipse.
    Ellipse,
    /// Segment.
    Line,
    /// Segment avec pointe de flèche.
    Arrow,
    /// Tracé à main levée.
    Ink,
    /// Texte libre posé sur la page.
    Text,
    /// Tampon (cadre + libellé).
    Stamp,
    /// Lien (lecture seule ici, voir le module `links`).
    Link,
    /// Champ de formulaire (Lot 4).
    Widget,
    /// Tout le reste (pièce jointe, son, 3D…).
    Other,
}

impl AnnotationKind {
    /// Nom affichable en français (libellés d'historique et CLI).
    pub fn label_fr(self) -> &'static str {
        match self {
            AnnotationKind::Note => "Note",
            AnnotationKind::Highlight => "Surlignage",
            AnnotationKind::Underline => "Soulignement",
            AnnotationKind::StrikeOut => "Barré",
            AnnotationKind::Squiggly => "Ondulé",
            AnnotationKind::Rect => "Rectangle",
            AnnotationKind::Ellipse => "Ellipse",
            AnnotationKind::Line => "Ligne",
            AnnotationKind::Arrow => "Flèche",
            AnnotationKind::Ink => "Encre",
            AnnotationKind::Text => "Texte",
            AnnotationKind::Stamp => "Tampon",
            AnnotationKind::Link => "Lien",
            AnnotationKind::Widget => "Champ",
            AnnotationKind::Other => "Annotation",
        }
    }

    /// Marquage de texte (porte des `/QuadPoints`).
    pub fn is_text_markup(self) -> bool {
        matches!(
            self,
            AnnotationKind::Highlight
                | AnnotationKind::Underline
                | AnnotationKind::StrikeOut
                | AnnotationKind::Squiggly
        )
    }

    /// Dessiné par nous comme `Ink` avec une apparence vectorielle.
    fn is_drawn_shape(self) -> bool {
        matches!(
            self,
            AnnotationKind::Rect
                | AnnotationKind::Ellipse
                | AnnotationKind::Line
                | AnnotationKind::Arrow
                | AnnotationKind::Ink
        )
    }
}

/// Couleur opaque (l'opacité est portée à part).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color {
    /// Rouge 0-255.
    pub r: u8,
    /// Vert 0-255.
    pub g: u8,
    /// Bleu 0-255.
    pub b: u8,
}

impl Color {
    /// Couleur PDFium avec l'opacité donnée (0-1).
    fn with_opacity(self, opacity: f32) -> PdfColor {
        PdfColor::new(
            self.r,
            self.g,
            self.b,
            (opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
        )
    }
}

impl From<PdfColor> for Color {
    fn from(c: PdfColor) -> Self {
        Color {
            r: c.red(),
            g: c.green(),
            b: c.blue(),
        }
    }
}

/// Style d'une annotation à créer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Style {
    /// Couleur du trait, du texte ou du marquage.
    pub color: Color,
    /// Couleur de remplissage (rectangle, ellipse), si demandée.
    pub fill: Option<Color>,
    /// Opacité 0-1 (exclu 0).
    pub opacity: f32,
    /// Épaisseur du trait en points (formes, encre).
    pub stroke_width: f32,
    /// Taille de police en points (texte, tampon).
    pub font_size: f32,
}

/// Géométrie d'une annotation, en points PDF (origine en bas à gauche).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Geometry {
    /// Rectangle (note : coin haut-gauche ; texte, tampon, rectangle, ellipse : emprise).
    Rect {
        /// L'emprise.
        rect: Rect,
    },
    /// Marquage de texte : un rectangle par ligne.
    Quads {
        /// Les rectangles (devenus `/QuadPoints`).
        rects: Vec<Rect>,
    },
    /// Segment (ligne, flèche : la pointe est en `to`).
    Line {
        /// Origine.
        from: [f32; 2],
        /// Extrémité.
        to: [f32; 2],
    },
    /// Tracés à main levée.
    Strokes {
        /// Une liste de points par trait.
        strokes: Vec<Vec<[f32; 2]>>,
    },
}

/// Une annotation à créer (vient de l'interface ou d'une reconstruction).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewAnnotation {
    /// Genre voulu.
    pub kind: AnnotationKind,
    /// Où.
    pub geometry: Geometry,
    /// Comment.
    pub style: Style,
    /// Commentaire, texte libre ou libellé de tampon.
    pub contents: String,
    /// Auteur (`/T`), vide pour ne rien écrire.
    pub author: String,
}

/// Une annotation telle que lue dans le document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    /// Position dans le tableau `/Annots` de la page (identifiant pour supprimer/déplacer).
    pub index: u16,
    /// Genre.
    pub kind: AnnotationKind,
    /// Rectangle englobant (`/Rect`), en points PDF.
    pub rect: Rect,
    /// Couleur principale (`/C`, ou celle de l'apparence), si connue.
    pub color: Option<Color>,
    /// Couleur de remplissage de l'apparence, si connue.
    pub fill: Option<Color>,
    /// Opacité 0-1 (`/CA`), 1 si absente.
    pub opacity: f32,
    /// Épaisseur du trait de l'apparence (0 si inconnue).
    pub stroke_width: f32,
    /// Taille de police de l'apparence (texte, tampon).
    pub font_size: Option<f32>,
    /// Auteur (`/T`).
    pub author: Option<String>,
    /// Commentaire ou texte (`/Contents`).
    pub contents: Option<String>,
    /// Date de création brute (`D:YYYYMMDD…`).
    pub created: Option<String>,
    /// Date de modification brute.
    pub modified: Option<String>,
    /// Quadrilatères des marquages de texte (`/QuadPoints`), un rectangle par ligne.
    pub quads: Vec<Rect>,
    /// Géométrie reconstruite depuis l'apparence (formes, encre, texte), si reconnue.
    pub geometry: Option<Geometry>,
}

impl Annotation {
    /// Reconstruit une demande de création équivalente (pour déplacer ou modifier) ;
    /// `None` si le genre n'est pas recréable (lien, champ, inconnu).
    pub fn to_new(&self) -> Option<NewAnnotation> {
        let geometry = match self.kind {
            AnnotationKind::Note => Geometry::Rect { rect: self.rect },
            k if k.is_text_markup() => Geometry::Quads {
                rects: self.quads.clone(),
            },
            AnnotationKind::Text | AnnotationKind::Stamp => Geometry::Rect { rect: self.rect },
            k if k.is_drawn_shape() => self.geometry.clone()?,
            _ => return None,
        };
        Some(NewAnnotation {
            kind: self.kind,
            geometry,
            style: Style {
                color: self.color.unwrap_or(Color { r: 0, g: 0, b: 0 }),
                fill: self.fill,
                opacity: self.opacity,
                stroke_width: if self.stroke_width > 0.0 {
                    self.stroke_width
                } else {
                    1.0
                },
                font_size: self.font_size.unwrap_or(12.0),
            },
            contents: self.contents.clone().unwrap_or_default(),
            author: self.author.clone().unwrap_or_default(),
        })
    }
}

fn invalid(msg: &str) -> CoreError {
    CoreError::InvalidOperation(msg.to_string())
}

/// Vérifie qu'une demande est réalisable et visible.
fn validate(new: &NewAnnotation) -> CoreResult<()> {
    let s = &new.style;
    if !(s.opacity > 0.0 && s.opacity <= 1.0) {
        return Err(invalid("l'opacité doit être comprise entre 0 (exclu) et 1"));
    }
    match new.kind {
        AnnotationKind::Link | AnnotationKind::Widget | AnnotationKind::Other => {
            return Err(invalid("ce genre d'annotation ne peut pas être créé"));
        }
        AnnotationKind::Highlight
        | AnnotationKind::Underline
        | AnnotationKind::StrikeOut
        | AnnotationKind::Squiggly => match &new.geometry {
            Geometry::Quads { rects } if !rects.is_empty() => {}
            _ => return Err(invalid("un marquage de texte exige au moins un rectangle")),
        },
        AnnotationKind::Note | AnnotationKind::Text | AnnotationKind::Stamp => {
            if !matches!(new.geometry, Geometry::Rect { .. }) {
                return Err(invalid("cette annotation exige un rectangle"));
            }
            if matches!(new.kind, AnnotationKind::Text | AnnotationKind::Stamp) && s.font_size < 4.0
            {
                return Err(invalid("la taille de police doit être d'au moins 4 points"));
            }
            if matches!(new.kind, AnnotationKind::Text | AnnotationKind::Stamp)
                && new.contents.trim().is_empty()
            {
                return Err(invalid("le texte ne peut pas être vide"));
            }
        }
        AnnotationKind::Rect | AnnotationKind::Ellipse => {
            if s.stroke_width <= 0.0 {
                return Err(invalid("l'épaisseur du trait doit être positive"));
            }
            if !matches!(new.geometry, Geometry::Rect { .. }) {
                return Err(invalid("cette forme exige un rectangle"));
            }
        }
        AnnotationKind::Line | AnnotationKind::Arrow => {
            if s.stroke_width <= 0.0 {
                return Err(invalid("l'épaisseur du trait doit être positive"));
            }
            match &new.geometry {
                Geometry::Line { from, to } if from != to => {}
                _ => return Err(invalid("une ligne exige deux points distincts")),
            }
        }
        AnnotationKind::Ink => {
            if s.stroke_width <= 0.0 {
                return Err(invalid("l'épaisseur du trait doit être positive"));
            }
            match &new.geometry {
                Geometry::Strokes { strokes } if strokes.iter().any(|s| s.len() >= 2) => {}
                _ => return Err(invalid("un tracé exige au moins deux points")),
            }
        }
    }
    Ok(())
}

impl Document {
    /// Les annotations de la page `page`, dans l'ordre du fichier (au plus
    /// [`MAX_ANNOTATIONS_PER_PAGE`]).
    pub fn annotations(&self, page: u16) -> CoreResult<Vec<Annotation>> {
        let _guard = engine::guard();
        let page = self
            .inner()
            .pages()
            .get(self.checked_index_unlocked(page)?)?;
        page.annotations()
            .iter()
            .take(MAX_ANNOTATIONS_PER_PAGE)
            .enumerate()
            .map(|(i, ann)| read_annotation(i as u16, &ann))
            .collect()
    }

    /// Ajoute une annotation à la page et retourne son index (en fin de tableau).
    ///
    /// Les couleurs sont posées **avant** l'apparence (`FPDFAnnot_SetColor` refuse ensuite).
    pub fn add_annotation(&mut self, page: u16, new: &NewAnnotation) -> CoreResult<u16> {
        validate(new)?;
        let _guard = engine::guard();
        let page_index = self.checked_index_unlocked(page)?;
        // La police intégrée est enregistrée sur le document (emprunt mutable, terminé avant
        // la suite) ; les objets texte y font référence par jeton.
        let font = matches!(new.kind, AnnotationKind::Text | AnnotationKind::Stamp)
            .then(|| self.inner_mut().fonts_mut().helvetica());
        let doc = self.inner();
        let mut pdf_page = doc.pages().get(page_index)?;
        let index = pdf_page.annotations().len();
        if index >= u16::MAX as usize {
            return Err(invalid("trop d'annotations sur cette page"));
        }
        let annots = pdf_page.annotations_mut();
        let stroke = new.style.color.with_opacity(new.style.opacity);
        match new.kind {
            AnnotationKind::Note => {
                let Geometry::Rect { rect } = &new.geometry else {
                    unreachable!("validé plus haut")
                };
                let mut a = annots.create_text_annotation(&new.contents)?;
                let icon = Rect {
                    x: rect.x,
                    y: rect.y + rect.h - appearance::NOTE_ICON_SIZE,
                    w: appearance::NOTE_ICON_SIZE,
                    h: appearance::NOTE_ICON_SIZE,
                };
                a.set_bounds(to_pdf_rect(&icon))?;
                a.set_stroke_color(stroke)?;
                set_meta(&mut a, new)?;
            }
            k if k.is_text_markup() => {
                let Geometry::Quads { rects } = &new.geometry else {
                    unreachable!("validé plus haut")
                };
                let bounds = union(rects);
                let quads: Vec<PdfQuadPoints> = rects
                    .iter()
                    .map(|r| PdfQuadPoints::from_rect(&to_pdf_rect(r)))
                    .collect();
                match k {
                    AnnotationKind::Highlight => {
                        let mut a = annots.create_highlight_annotation()?;
                        a.set_bounds(to_pdf_rect(&bounds))?;
                        a.set_stroke_color(stroke)?;
                        for q in quads {
                            a.attachment_points_mut()
                                .create_attachment_point_at_end(q)?;
                        }
                        set_meta(&mut a, new)?;
                    }
                    AnnotationKind::Underline => {
                        let mut a = annots.create_underline_annotation()?;
                        a.set_bounds(to_pdf_rect(&bounds))?;
                        a.set_stroke_color(stroke)?;
                        for q in quads {
                            a.attachment_points_mut()
                                .create_attachment_point_at_end(q)?;
                        }
                        set_meta(&mut a, new)?;
                    }
                    AnnotationKind::StrikeOut => {
                        let mut a = annots.create_strikeout_annotation()?;
                        a.set_bounds(to_pdf_rect(&bounds))?;
                        a.set_stroke_color(stroke)?;
                        for q in quads {
                            a.attachment_points_mut()
                                .create_attachment_point_at_end(q)?;
                        }
                        set_meta(&mut a, new)?;
                    }
                    _ => {
                        let mut a = annots.create_squiggly_annotation()?;
                        a.set_bounds(to_pdf_rect(&bounds))?;
                        a.set_stroke_color(stroke)?;
                        for q in quads {
                            a.attachment_points_mut()
                                .create_attachment_point_at_end(q)?;
                        }
                        set_meta(&mut a, new)?;
                    }
                }
            }
            k if k.is_drawn_shape() => {
                let bounds = appearance::shape_bounds(new);
                let mut a = annots.create_ink_annotation()?;
                a.set_bounds(to_pdf_rect(&bounds))?;
                a.set_stroke_color(stroke)?;
                set_meta(&mut a, new)?;
                appearance::draw_shape(doc, a.objects_mut(), new)?;
            }
            _ => {
                // Text | Stamp
                let Geometry::Rect { rect } = &new.geometry else {
                    unreachable!("validé plus haut")
                };
                let mut a = annots.create_stamp_annotation()?;
                a.set_bounds(to_pdf_rect(rect))?;
                a.set_stroke_color(stroke)?;
                set_meta(&mut a, new)?;
                #[allow(clippy::expect_used)]
                let font = font.expect("jeton de police créé pour Text/Stamp");
                appearance::draw_text(doc, a.objects_mut(), new, font)?;
            }
        }
        Ok(index as u16)
    }

    /// Supprime l'annotation `index` de la page.
    pub fn delete_annotation(&mut self, page: u16, index: u16) -> CoreResult<()> {
        let _guard = engine::guard();
        let page_index = self.checked_index_unlocked(page)?;
        let mut pdf_page = self.inner().pages().get(page_index)?;
        let count = pdf_page.annotations().len();
        if usize::from(index) >= count {
            return Err(CoreError::InvalidOperation(format!(
                "annotation {index} introuvable (la page en a {count})"
            )));
        }
        let annots = pdf_page.annotations_mut();
        let ann = annots.get(usize::from(index))?;
        annots.delete_annotation(ann)?;
        Ok(())
    }

    /// Déplace / redimensionne l'annotation `index` dans `rect` et retourne son nouvel index.
    ///
    /// Une note change simplement de `/Rect`. Une forme, un texte ou un tampon est **recréé**
    /// avec sa géométrie mise à l'échelle (son apparence est en coordonnées de page ; changer
    /// `/Rect` seul la laisserait sur place ou la déformerait selon les lecteurs). Les
    /// marquages de texte ne se déplacent pas (ils suivent le texte).
    pub fn set_annotation_rect(&mut self, page: u16, index: u16, rect: Rect) -> CoreResult<u16> {
        let current = self
            .annotations(page)?
            .into_iter()
            .find(|a| a.index == index)
            .ok_or_else(|| invalid("annotation introuvable"))?;
        if current.kind == AnnotationKind::Note {
            let _guard = engine::guard();
            let page_index = self.checked_index_unlocked(page)?;
            let pdf_page = self.inner().pages().get(page_index)?;
            let mut ann = pdf_page.annotations().get(usize::from(index))?;
            let icon = Rect {
                x: rect.x,
                y: rect.y + rect.h - appearance::NOTE_ICON_SIZE,
                w: appearance::NOTE_ICON_SIZE,
                h: appearance::NOTE_ICON_SIZE,
            };
            ann.set_bounds(to_pdf_rect(&icon))?;
            return Ok(index);
        }
        if current.kind.is_text_markup() {
            return Err(invalid("un marquage de texte ne se déplace pas"));
        }
        let mut new = current
            .to_new()
            .ok_or_else(|| invalid("cette annotation ne peut pas être déplacée"))?;
        // Le `/Rect` d'une forme déborde du dessin de l'épaisseur du trait (+ 1 pt) : la
        // géométrie se transforme entre les deux emprises **intérieures**.
        let pad = if current.kind.is_drawn_shape() {
            new.style.stroke_width / 2.0 + 1.0
        } else {
            0.0
        };
        let inner = |r: &Rect| Rect {
            x: r.x + pad,
            y: r.y + pad,
            w: (r.w - 2.0 * pad).max(1.0),
            h: (r.h - 2.0 * pad).max(1.0),
        };
        new.geometry = appearance::rescale(&new.geometry, &inner(&current.rect), &inner(&rect));
        self.delete_annotation(page, index)?;
        self.add_annotation(page, &new)
    }
}

/// Métadonnées communes : contenu, auteur, date de création, impression.
fn set_meta<A: PdfPageAnnotationCommon>(a: &mut A, new: &NewAnnotation) -> CoreResult<()> {
    if !new.contents.is_empty() {
        a.set_contents(&new.contents)?;
    }
    if !new.author.trim().is_empty() {
        a.set_creator(new.author.trim())?;
    }
    a.set_creation_date(Utc::now())?;
    a.set_is_printed(true)?;
    Ok(())
}

pub(crate) fn to_pdf_rect(r: &Rect) -> PdfRect {
    PdfRect::new(
        PdfPoints::new(r.y),
        PdfPoints::new(r.x),
        PdfPoints::new(r.y + r.h),
        PdfPoints::new(r.x + r.w),
    )
}

/// Rectangle englobant d'une liste (vide → rectangle nul).
pub(crate) fn union(rects: &[Rect]) -> Rect {
    let mut it = rects.iter();
    let Some(first) = it.next() else {
        return Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        };
    };
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x + first.w, first.y + first.h);
    for r in it {
        x0 = x0.min(r.x);
        y0 = y0.min(r.y);
        x1 = x1.max(r.x + r.w);
        y1 = y1.max(r.y + r.h);
    }
    Rect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    }
}

/// Genre d'une annotation d'après son sous-type PDF ; `Ink` et `Stamp` sont affinés par
/// l'analyse de leur apparence (module `appearance`).
fn kind_of(ann: &PdfPageAnnotation) -> AnnotationKind {
    match ann.annotation_type() {
        PdfPageAnnotationType::Text => AnnotationKind::Note,
        PdfPageAnnotationType::Highlight => AnnotationKind::Highlight,
        PdfPageAnnotationType::Underline => AnnotationKind::Underline,
        PdfPageAnnotationType::Strikeout => AnnotationKind::StrikeOut,
        PdfPageAnnotationType::Squiggly => AnnotationKind::Squiggly,
        PdfPageAnnotationType::Square => AnnotationKind::Rect,
        PdfPageAnnotationType::Circle => AnnotationKind::Ellipse,
        PdfPageAnnotationType::Line => AnnotationKind::Line,
        PdfPageAnnotationType::Ink => AnnotationKind::Ink,
        PdfPageAnnotationType::FreeText => AnnotationKind::Text,
        PdfPageAnnotationType::Stamp => AnnotationKind::Stamp,
        PdfPageAnnotationType::Link => AnnotationKind::Link,
        PdfPageAnnotationType::Widget | PdfPageAnnotationType::XfaWidget => AnnotationKind::Widget,
        PdfPageAnnotationType::Unknown
        | PdfPageAnnotationType::Polygon
        | PdfPageAnnotationType::Polyline
        | PdfPageAnnotationType::Caret
        | PdfPageAnnotationType::Popup
        | PdfPageAnnotationType::FileAttachment
        | PdfPageAnnotationType::Sound
        | PdfPageAnnotationType::Movie
        | PdfPageAnnotationType::Screen
        | PdfPageAnnotationType::PrinterMark
        | PdfPageAnnotationType::TrapNet
        | PdfPageAnnotationType::Watermark
        | PdfPageAnnotationType::ThreeD
        | PdfPageAnnotationType::RichMedia
        | PdfPageAnnotationType::Redacted => AnnotationKind::Other,
    }
}

/// Couleur et opacité de l'annotation.
///
/// **Piège réel** (pdfium-render 0.9.4) : `stroke_color()` sur une annotation qui possède
/// un flux d'apparence `/AP` retombe sur `FPDFPageObj_GetStrokeColor` avec le handle de
/// l'annotation transtypé en objet de page : violation d'accès (plantage du processus).
/// La couleur vient donc d'abord des objets de l'apparence. `/C` n'est lu directement que
/// pour une **note** sans objet d'apparence (seul genre où cette couleur compte et où une
/// apparence est rare) ; le cas « `/AP` présent mais vide » reste un risque résiduel connu.
/// Sans `/C`, PDFium répond noir (valeur par défaut de la norme).
fn color_of(ann: &PdfPageAnnotation, kind: AnnotationKind) -> (Option<Color>, f32) {
    for obj in ann.objects().iter() {
        let stroked = obj
            .as_path_object()
            .and_then(|p| p.is_stroked().ok())
            .unwrap_or(false);
        let color = if stroked {
            obj.stroke_color().ok()
        } else {
            obj.fill_color().ok()
        };
        if let Some(c) = color {
            return (Some(c.into()), f32::from(c.alpha()) / 255.0);
        }
    }
    if kind == AnnotationKind::Note
        && ann.objects().is_empty()
        && let Ok(c) = ann.stroke_color()
    {
        return (Some(c.into()), f32::from(c.alpha()) / 255.0);
    }
    (None, 1.0)
}

fn truncate(s: Option<String>) -> Option<String> {
    s.filter(|s| !s.is_empty())
        .map(|s| s.chars().take(MAX_STRING_CHARS).collect())
}

fn read_annotation(index: u16, ann: &PdfPageAnnotation) -> CoreResult<Annotation> {
    let subtype = kind_of(ann);
    let inspected = appearance::inspect(ann, subtype);
    let (color, opacity) = color_of(ann, inspected.kind);
    let quads = ann
        .attachment_points()
        .iter()
        .take(MAX_QUADS_PER_ANNOTATION)
        .map(|q| Rect {
            x: q.left().value,
            y: q.bottom().value,
            w: q.width().value,
            h: q.height().value,
        })
        .collect();
    Ok(Annotation {
        index,
        kind: inspected.kind,
        rect: Rect::from_pdf(ann.bounds()?),
        color,
        fill: inspected.fill,
        opacity,
        stroke_width: inspected.stroke_width,
        font_size: inspected.font_size,
        author: truncate(ann.creator()),
        contents: truncate(ann.contents()),
        created: truncate(ann.creation_date()),
        modified: truncate(ann.modification_date()),
        quads,
        geometry: inspected.geometry,
    })
}
