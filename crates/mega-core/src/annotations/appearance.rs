// SPDX-License-Identifier: GPL-3.0-or-later
//! Apparences dessinées par nous (formes, encre, texte, tampon) et leur relecture.
//!
//! \[Apprentissage\] **Pattern builder, version PDFium** : un objet chemin se construit pas
//! à pas (`new` au point de départ, puis `line_to` / `bezier_to`, puis `close_path`), un
//! objet texte se crée puis se positionne (`translate`). Chaque étape retourne un
//! `Result` : on enchaîne avec `?` et l'annotation reste cohérente ou l'erreur remonte.

use pdfium_render::prelude::{
    PdfColor, PdfDocument, PdfFontToken, PdfPageAnnotation, PdfPageAnnotationCommon,
    PdfPageAnnotationObjects, PdfPageObjectCommon, PdfPageObjectsCommon, PdfPagePathObject,
    PdfPageTextObject, PdfPathSegmentType, PdfPathSegments, PdfPoints,
};

use super::{AnnotationKind, Color, Geometry, NewAnnotation, to_pdf_rect, union};
use crate::error::CoreResult;
use crate::geometry::Rect;

/// Côté de l'icône d'une note, en points.
pub const NOTE_ICON_SIZE: f32 = 20.0;

fn pdf_color(c: Color, opacity: f32) -> PdfColor {
    c.with_opacity(opacity)
}

fn pt(v: f32) -> PdfPoints {
    PdfPoints::new(v)
}

/// Longueur de la pointe d'une flèche pour une épaisseur donnée.
fn arrow_head_len(width: f32) -> f32 {
    (4.0 * width).max(8.0)
}

/// Rectangle englobant d'une forme, trait compris.
pub(crate) fn shape_bounds(new: &NewAnnotation) -> Rect {
    let pad = new.style.stroke_width / 2.0 + 1.0;
    let base = match &new.geometry {
        Geometry::Rect { rect } => *rect,
        Geometry::Quads { rects } => union(rects),
        Geometry::Line { from, to } => {
            let extra = if new.kind == AnnotationKind::Arrow {
                arrow_head_len(new.style.stroke_width)
            } else {
                0.0
            };
            let (x0, x1) = (from[0].min(to[0]) - extra, from[0].max(to[0]) + extra);
            let (y0, y1) = (from[1].min(to[1]) - extra, from[1].max(to[1]) + extra);
            Rect {
                x: x0,
                y: y0,
                w: x1 - x0,
                h: y1 - y0,
            }
        }
        Geometry::Strokes { strokes } => {
            let pts = strokes.iter().flatten();
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for p in pts {
                x0 = x0.min(p[0]);
                y0 = y0.min(p[1]);
                x1 = x1.max(p[0]);
                y1 = y1.max(p[1]);
            }
            Rect {
                x: x0,
                y: y0,
                w: (x1 - x0).max(0.0),
                h: (y1 - y0).max(0.0),
            }
        }
    };
    Rect {
        x: base.x - pad,
        y: base.y - pad,
        w: base.w + 2.0 * pad,
        h: base.h + 2.0 * pad,
    }
}

/// Dessine rectangle, ellipse, ligne, flèche ou encre dans l'apparence d'une annotation `Ink`.
pub(crate) fn draw_shape<'a>(
    doc: &PdfDocument<'a>,
    objects: &mut PdfPageAnnotationObjects<'a>,
    new: &NewAnnotation,
) -> CoreResult<()> {
    let s = &new.style;
    let stroke = pdf_color(s.color, s.opacity);
    let fill = s.fill.map(|f| pdf_color(f, s.opacity));
    let width = pt(s.stroke_width);
    match (&new.kind, &new.geometry) {
        (AnnotationKind::Rect, Geometry::Rect { rect }) => {
            let obj = PdfPagePathObject::new_rect(
                doc,
                to_pdf_rect(rect),
                Some(stroke),
                Some(width),
                fill,
            )?;
            objects.add_path_object(obj)?;
        }
        (AnnotationKind::Ellipse, Geometry::Rect { rect }) => {
            let obj = PdfPagePathObject::new_ellipse(
                doc,
                to_pdf_rect(rect),
                Some(stroke),
                Some(width),
                fill,
            )?;
            objects.add_path_object(obj)?;
        }
        (AnnotationKind::Line, Geometry::Line { from, to }) => {
            let obj = PdfPagePathObject::new_line(
                doc,
                pt(from[0]),
                pt(from[1]),
                pt(to[0]),
                pt(to[1]),
                stroke,
                width,
            )?;
            objects.add_path_object(obj)?;
        }
        (AnnotationKind::Arrow, Geometry::Line { from, to }) => {
            let shaft = PdfPagePathObject::new_line(
                doc,
                pt(from[0]),
                pt(from[1]),
                pt(to[0]),
                pt(to[1]),
                stroke,
                width,
            )?;
            objects.add_path_object(shaft)?;
            // Pointe : deux traits à 30° de part et d'autre de l'axe, depuis l'extrémité.
            let len = arrow_head_len(s.stroke_width);
            let angle = (to[1] - from[1]).atan2(to[0] - from[0]);
            let (a1, a2) = (angle + 2.618_f32, angle - 2.618_f32); // ±150°
            let mut head =
                PdfPagePathObject::new(doc, pt(to[0]), pt(to[1]), Some(stroke), Some(width), None)?;
            head.line_to(pt(to[0] + len * a1.cos()), pt(to[1] + len * a1.sin()))?;
            head.move_to(pt(to[0]), pt(to[1]))?;
            head.line_to(pt(to[0] + len * a2.cos()), pt(to[1] + len * a2.sin()))?;
            objects.add_path_object(head)?;
        }
        (AnnotationKind::Ink, Geometry::Strokes { strokes }) => {
            for stroke_pts in strokes.iter().filter(|s| s.len() >= 2) {
                let mut obj = PdfPagePathObject::new(
                    doc,
                    pt(stroke_pts[0][0]),
                    pt(stroke_pts[0][1]),
                    Some(stroke),
                    Some(width),
                    None,
                )?;
                for p in &stroke_pts[1..] {
                    obj.line_to(pt(p[0]), pt(p[1]))?;
                }
                objects.add_path_object(obj)?;
            }
        }
        _ => {
            return Err(crate::error::CoreError::InvalidOperation(
                "géométrie incompatible avec ce genre d'annotation".to_string(),
            ));
        }
    }
    Ok(())
}

/// Dessine un texte libre (lignes) ou un tampon (cadre + libellé centré) dans un `Stamp`.
pub(crate) fn draw_text<'a>(
    doc: &PdfDocument<'a>,
    objects: &mut PdfPageAnnotationObjects<'a>,
    new: &NewAnnotation,
    font: PdfFontToken,
) -> CoreResult<()> {
    let Geometry::Rect { rect } = &new.geometry else {
        return Err(crate::error::CoreError::InvalidOperation(
            "un texte exige un rectangle".to_string(),
        ));
    };
    let s = &new.style;
    let color = pdf_color(s.color, s.opacity);
    match new.kind {
        AnnotationKind::Stamp => {
            // Cadre.
            let frame = PdfPagePathObject::new_rect(
                doc,
                to_pdf_rect(&Rect {
                    x: rect.x + 1.0,
                    y: rect.y + 1.0,
                    w: (rect.w - 2.0).max(1.0),
                    h: (rect.h - 2.0).max(1.0),
                }),
                Some(color),
                Some(pt(2.0)),
                None,
            )?;
            objects.add_path_object(frame)?;
            // Libellé centré, taille bornée par la hauteur du cadre.
            let size = s.font_size.min(rect.h * 0.6).max(4.0);
            let mut text = PdfPageTextObject::new(doc, new.contents.trim(), font, pt(size))?;
            text.set_fill_color(color)?;
            let b = text.bounds()?;
            let tw = b.width().value;
            let x = rect.x + (rect.w - tw) / 2.0;
            let y = rect.y + (rect.h - size * 0.7) / 2.0;
            text.translate(pt(x - b.left().value), pt(y - b.bottom().value))?;
            objects.add_text_object(text)?;
        }
        _ => {
            let size = s.font_size;
            let top = rect.y + rect.h;
            for (i, line) in new.contents.lines().enumerate() {
                if line.trim().is_empty() {
                    continue;
                }
                let baseline = top - size * 0.85 - (i as f32) * size * 1.2;
                let mut text = PdfPageTextObject::new(doc, line, font, pt(size))?;
                text.set_fill_color(color)?;
                let b = text.bounds()?;
                text.translate(
                    pt(rect.x + 2.0 - b.left().value),
                    pt(baseline - b.bottom().value),
                )?;
                objects.add_text_object(text)?;
            }
        }
    }
    Ok(())
}

/// Ce que l'apparence révèle d'une annotation existante.
pub(crate) struct Inspected {
    pub kind: AnnotationKind,
    pub geometry: Option<Geometry>,
    pub fill: Option<Color>,
    pub stroke_width: f32,
    pub font_size: Option<f32>,
}

/// Affine le genre d'une `Ink` ou d'un `Stamp` d'après ses objets d'apparence et en
/// reconstruit la géométrie. Les autres sous-types sont rendus tels quels.
pub(crate) fn inspect(ann: &PdfPageAnnotation, subtype: AnnotationKind) -> Inspected {
    let mut out = Inspected {
        kind: subtype,
        geometry: None,
        fill: None,
        stroke_width: 0.0,
        font_size: None,
    };
    if !matches!(subtype, AnnotationKind::Ink | AnnotationKind::Stamp) {
        return out;
    }
    let mut paths: Vec<Vec<(PdfPathSegmentType, f32, f32, bool)>> = Vec::new();
    let mut text_count = 0usize;
    for obj in ann.objects().iter() {
        if let Some(p) = obj.as_path_object() {
            if out.stroke_width == 0.0 {
                out.stroke_width = p.stroke_width().map(|w| w.value).unwrap_or(0.0);
            }
            if out.fill.is_none()
                && p.fill_mode()
                    .map(|m| m != pdfium_render::prelude::PdfPathFillMode::None)
                    .unwrap_or(false)
                && let Ok(c) = p.fill_color()
            {
                out.fill = Some(c.into());
            }
            paths.push(
                p.segments()
                    .iter()
                    .map(|s| (s.segment_type(), s.x().value, s.y().value, s.is_close()))
                    .collect(),
            );
        } else if let Some(t) = obj.as_text_object() {
            text_count += 1;
            if out.font_size.is_none() {
                out.font_size = Some(t.scaled_font_size().value);
            }
        }
    }
    let bounds = ann.bounds().map(Rect::from_pdf).unwrap_or(Rect {
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    });
    if subtype == AnnotationKind::Stamp {
        out.kind = if text_count > 0 && paths.is_empty() {
            AnnotationKind::Text
        } else {
            AnnotationKind::Stamp
        };
        out.geometry = Some(Geometry::Rect { rect: bounds });
        return out;
    }
    // Ink : classer d'après la forme des chemins.
    let count = |segs: &[(PdfPathSegmentType, f32, f32, bool)], t: PdfPathSegmentType| {
        segs.iter().filter(|s| s.0 == t).count()
    };
    let points = |segs: &[(PdfPathSegmentType, f32, f32, bool)]| -> Vec<[f32; 2]> {
        segs.iter().map(|s| [s.1, s.2]).collect()
    };
    let object_bounds = |i: usize| -> Option<Rect> {
        ann.objects()
            .get(i)
            .ok()
            .and_then(|o| o.bounds().ok())
            .map(|q| Rect {
                x: q.left().value,
                y: q.bottom().value,
                w: q.width().value,
                h: q.height().value,
            })
    };
    match paths.as_slice() {
        [single] => {
            let lines = count(single, PdfPathSegmentType::LineTo);
            let beziers = count(single, PdfPathSegmentType::BezierTo);
            let moves = count(single, PdfPathSegmentType::MoveTo);
            let pts = points(single);
            if beziers > 0 && lines == 0 {
                out.kind = AnnotationKind::Ellipse;
                out.geometry = object_bounds(0).map(|rect| Geometry::Rect { rect });
            } else if moves == 1 && lines == 1 && pts.len() == 2 {
                out.kind = AnnotationKind::Line;
                out.geometry = Some(Geometry::Line {
                    from: pts[0],
                    to: pts[1],
                });
            } else if moves == 1 && (3..=4).contains(&lines) && is_axis_aligned_rect(&pts) {
                out.kind = AnnotationKind::Rect;
                out.geometry = object_bounds(0).map(|rect| Geometry::Rect { rect });
            } else {
                out.kind = AnnotationKind::Ink;
                out.geometry = Some(Geometry::Strokes {
                    strokes: strokes_of(&paths),
                });
            }
        }
        [shaft, head]
            if count(shaft, PdfPathSegmentType::LineTo) == 1
                && count(head, PdfPathSegmentType::LineTo) == 2 =>
        {
            let pts = points(shaft);
            out.kind = AnnotationKind::Arrow;
            out.geometry = Some(Geometry::Line {
                from: pts[0],
                to: pts[1],
            });
        }
        _ => {
            out.kind = AnnotationKind::Ink;
            out.geometry = Some(Geometry::Strokes {
                strokes: strokes_of(&paths),
            });
        }
    }
    out
}

fn is_axis_aligned_rect(pts: &[[f32; 2]]) -> bool {
    if pts.len() < 4 {
        return false;
    }
    let eps = 0.01;
    pts.windows(2)
        .all(|w| (w[0][0] - w[1][0]).abs() < eps || (w[0][1] - w[1][1]).abs() < eps)
}

/// Découpe les chemins en traits : un `MoveTo` commence un trait, les autres points le prolongent.
fn strokes_of(paths: &[Vec<(PdfPathSegmentType, f32, f32, bool)>]) -> Vec<Vec<[f32; 2]>> {
    let mut strokes = Vec::new();
    for path in paths {
        let mut current: Vec<[f32; 2]> = Vec::new();
        for (t, x, y, _) in path {
            if *t == PdfPathSegmentType::MoveTo && !current.is_empty() {
                strokes.push(std::mem::take(&mut current));
            }
            current.push([*x, *y]);
        }
        if !current.is_empty() {
            strokes.push(current);
        }
    }
    strokes
}

/// Applique à une géométrie la transformation affine qui envoie `from` sur `to`.
pub(crate) fn rescale(geometry: &Geometry, from: &Rect, to: &Rect) -> Geometry {
    let sx = if from.w > 0.0 { to.w / from.w } else { 1.0 };
    let sy = if from.h > 0.0 { to.h / from.h } else { 1.0 };
    let map =
        |p: &[f32; 2]| -> [f32; 2] { [to.x + (p[0] - from.x) * sx, to.y + (p[1] - from.y) * sy] };
    let map_rect = |r: &Rect| -> Rect {
        let [x, y] = map(&[r.x, r.y]);
        Rect {
            x,
            y,
            w: r.w * sx,
            h: r.h * sy,
        }
    };
    match geometry {
        Geometry::Rect { rect } => Geometry::Rect {
            rect: map_rect(rect),
        },
        Geometry::Quads { rects } => Geometry::Quads {
            rects: rects.iter().map(map_rect).collect(),
        },
        Geometry::Line { from: a, to: b } => Geometry::Line {
            from: map(a),
            to: map(b),
        },
        Geometry::Strokes { strokes } => Geometry::Strokes {
            strokes: strokes
                .iter()
                .map(|s| s.iter().map(map).collect())
                .collect(),
        },
    }
}
