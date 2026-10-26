// SPDX-License-Identifier: GPL-3.0-or-later
//! Annotations : lecture du modèle (sous-type, position, couleur, auteur, contenu, dates).
//!
//! \[Apprentissage\] **`enum` riche et `match` exhaustif** : PDFium distingue une trentaine
//! de sous-types d'annotations ; nous les ramenons à une poignée de « genres » utiles à
//! l'interface ([`AnnotationKind`]). Le `match` sur le sous-type est exhaustif : si une
//! future version de pdfium-render ajoute un sous-type, le compilateur nous le signalera.

use pdfium_render::prelude::{
    PdfColor, PdfPageAnnotation, PdfPageAnnotationCommon, PdfPageAnnotationType,
    PdfPageObjectCommon, PdfPageObjectsCommon,
};
use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::engine;
use crate::error::CoreResult;
use crate::geometry::Rect;

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

impl From<PdfColor> for Color {
    fn from(c: PdfColor) -> Self {
        Color {
            r: c.red(),
            g: c.green(),
            b: c.blue(),
        }
    }
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
    /// Opacité 0-1 (`/CA`), 1 si absente.
    pub opacity: f32,
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
}

impl Document {
    /// Les annotations de la page `page`, dans l'ordre du fichier.
    pub fn annotations(&self, page: u16) -> CoreResult<Vec<Annotation>> {
        let _guard = engine::guard();
        let page = self
            .inner()
            .pages()
            .get(self.checked_index_unlocked(page)?)?;
        page.annotations()
            .iter()
            .enumerate()
            .map(|(i, ann)| read_annotation(i as u16, &ann))
            .collect()
    }
}

/// Genre d'une annotation d'après son sous-type PDF ; `Ink` et `Stamp` sont affinés par
/// l'analyse de leur apparence (module `appearance`, Lot 3 tâche 2).
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
/// On ne l'appelle donc que si l'annotation n'a **aucun** objet d'apparence et si son genre
/// porte une couleur utile ; sinon la couleur vient du premier objet de l'apparence.
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
    if ann.objects().is_empty()
        && styled(kind)
        && let Ok(c) = ann.stroke_color()
    {
        return (Some(c.into()), f32::from(c.alpha()) / 255.0);
    }
    (None, 1.0)
}

/// Genres dont la couleur `/C` a un sens pour l'interface (les liens et champs de
/// formulaire portent presque toujours une apparence : on ne tente pas de lire `/C`).
fn styled(kind: AnnotationKind) -> bool {
    !matches!(
        kind,
        AnnotationKind::Link | AnnotationKind::Widget | AnnotationKind::Other
    )
}

fn read_annotation(index: u16, ann: &PdfPageAnnotation) -> CoreResult<Annotation> {
    let kind = kind_of(ann);
    let (color, opacity) = color_of(ann, kind);
    let quads = ann
        .attachment_points()
        .iter()
        .map(|q| Rect {
            x: q.left().value,
            y: q.bottom().value,
            w: q.width().value,
            h: q.height().value,
        })
        .collect();
    Ok(Annotation {
        index,
        kind,
        rect: Rect::from_pdf(ann.bounds()?),
        color,
        opacity,
        author: ann.creator().filter(|s| !s.is_empty()),
        contents: ann.contents().filter(|s| !s.is_empty()),
        created: ann.creation_date().filter(|s| !s.is_empty()),
        modified: ann.modification_date().filter(|s| !s.is_empty()),
        quads,
    })
}
