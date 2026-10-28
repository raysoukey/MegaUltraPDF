// SPDX-License-Identifier: GPL-3.0-or-later
//! Apparence d'une signature **visible** (Lot 10) : le « tampon » dessiné dans le widget de
//! signature, un formulaire XObject `/AP /N` (§12.7.5.5, §12.5.5).
//!
//! Le tampon montre, dans un cadre fin : l'image facultative du signataire à gauche, puis le
//! texte « Signé numériquement par », le nom déclaré par le certificat, la date (UTC) et le
//! motif et le lieu s'ils sont donnés. Le texte est en Helvetica, ou en police Unicode
//! incorporée s'il sort de WinAnsi.
//!
//! **Page tournée** : la boîte `/BBox` a les dimensions **affichées** et la `/Matrix` du
//! formulaire porte la rotation de la page (la même que [`PageFrame::matrix_to_user_space`],
//! sans la translation) : le lecteur ramène la boîte transformée sur `/Rect`, et la rotation
//! de la page la remet droite à l'écran.
//!
//! \[Apprentissage\] Ce tampon ne fait **pas** partie de ce que la signature garantit : c'est une
//! image de la signature, pas la signature elle-même. Le panneau des signatures, lui, vérifie
//! le contenu cryptographique (voir `docs/apprentissage/lot-8.md`).
//!
//! [`PageFrame::matrix_to_user_space`]: crate::geometry::PageFrame::matrix_to_user_space

use image::DynamicImage;
use lopdf::{Dictionary, Document as LoDocument, Object, ObjectId, Stream};

use super::SignOptions;
use crate::error::CoreResult;
use crate::fonts;
use crate::fonts::unicode::Type0Writer;
use crate::geometry::Rect;
use crate::overlay::{helvetica_font, hex_win_ansi, num, text_width};

/// Interligne, en multiple de la taille de police.
const LINE_HEIGHT: f32 = 1.2;
/// Taille de police maximale du texte du tampon (points).
const MAX_FONT_SIZE: f32 = 14.0;
/// Taille de police minimale (en dessous, le texte serait illisible : il est alors coupé).
const MIN_FONT_SIZE: f32 = 4.0;

/// Où et avec quoi dessiner la signature, résolu **avant** la phase `lopdf` (PDFium connaît
/// la page : sa taille affichée et sa rotation).
pub(crate) struct Placement {
    /// Page visée (index à partir de 0).
    pub page_index: u16,
    /// Rectangle du widget, en espace **utilisateur** (celui de `/Rect`).
    pub user_rect: Rect,
    /// Largeur affichée du tampon (points).
    pub width: f32,
    /// Hauteur affichée du tampon (points).
    pub height: f32,
    /// Partie linéaire `[a b c d]` de la matrice espace affiché → utilisateur ; `None` pour
    /// une page droite.
    pub rotation: Option<[f32; 4]>,
    /// Image facultative du signataire, déjà décodée.
    pub image: Option<DynamicImage>,
}

/// Les lignes du tampon : mention, signataire, date, motif et lieu s'ils sont donnés.
pub(crate) fn lines(signer: &str, now: i64, options: &SignOptions) -> Vec<String> {
    let mut out = vec!["Signé numériquement par".to_string(), signer.to_string()];
    let date = chrono::DateTime::from_timestamp(now, 0)
        .map(|d| d.format("Date : %d/%m/%Y %H:%M UTC").to_string())
        .unwrap_or_default();
    if !date.is_empty() {
        out.push(date);
    }
    for (label, value) in [("Motif", &options.reason), ("Lieu", &options.location)] {
        if let Some(v) = value.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            out.push(format!("{label} : {v}"));
        }
    }
    out
}

/// Largeur de `line` à `size` points dans la police du tampon (Unicode si `unicode`).
fn line_width(line: &str, unicode: bool, size: f32) -> f32 {
    if unicode {
        crate::fonts::unicode::UnicodeFont::embedded().text_width(line, size)
    } else {
        text_width(line, size)
    }
}

/// La plus grande taille (entre 4 et 14 pt) à laquelle toutes les `lines` tiennent dans
/// `text_w` × `text_h`, mesurées dans la police qui les dessinera.
fn font_size(lines: &[String], unicode: bool, text_w: f32, text_h: f32) -> f32 {
    let count = lines.len().max(1) as f32;
    let by_height = text_h / (count * LINE_HEIGHT);
    let by_width = lines
        .iter()
        .map(|l| line_width(l, unicode, 1.0))
        .filter(|per_point| *per_point > 0.0)
        .map(|per_point| text_w / per_point)
        .fold(f32::MAX, f32::min);
    by_height.min(by_width).clamp(MIN_FONT_SIZE, MAX_FONT_SIZE)
}

/// Flux image (RVB 8 bits, comprimé) et, si l'image a de la transparence, son `/SMask`.
fn image_xobject(doc: &mut LoDocument, image: &DynamicImage) -> ObjectId {
    let image_dict = |w: u32, h: u32, space: &str| {
        Dictionary::from_iter(vec![
            ("Type", Object::Name(b"XObject".to_vec())),
            ("Subtype", Object::Name(b"Image".to_vec())),
            ("Width", Object::Integer(i64::from(w))),
            ("Height", Object::Integer(i64::from(h))),
            ("ColorSpace", Object::Name(space.as_bytes().to_vec())),
            ("BitsPerComponent", Object::Integer(8)),
        ])
    };
    let (w, h) = (image.width(), image.height());
    let mut dict = image_dict(w, h, "DeviceRGB");
    if image.color().has_alpha() {
        let alpha: Vec<u8> = image.to_rgba8().pixels().map(|p| p.0[3]).collect();
        let mut mask = Stream::new(image_dict(w, h, "DeviceGray"), alpha);
        let _ = mask.compress();
        dict.set("SMask", Object::Reference(doc.add_object(mask)));
    }
    let mut stream = Stream::new(dict, image.to_rgb8().into_raw());
    let _ = stream.compress();
    doc.add_object(stream)
}

/// Écrit l'apparence (formulaire XObject) et rend sa référence, à poser sous `/AP /N`.
pub(crate) fn write(
    doc: &mut LoDocument,
    placement: &Placement,
    lines: &[String],
) -> CoreResult<ObjectId> {
    let (w, h) = (placement.width, placement.height);
    let pad = (h * 0.08).clamp(1.0, 4.0);
    let mut ops = String::new();
    let mut resources = Dictionary::new();
    // Cadre fin, gris.
    ops.push_str(&format!(
        "q 0.45 G 0.75 w 0.375 0.375 {} {} re S Q\n",
        num(w - 0.75),
        num(h - 0.75)
    ));
    // Image à gauche, ajustée dans un carré de la hauteur du tampon (au plus 40 % de sa
    // largeur), centrée.
    let mut text_x = pad;
    if let Some(image) = &placement.image {
        let slot = Rect {
            x: pad,
            y: pad,
            w: (h - 2.0 * pad).min(w * 0.4).max(1.0),
            h: (h - 2.0 * pad).max(1.0),
        };
        let fitted = crate::content::image::fit(image.width() as f32, image.height() as f32, &slot);
        let id = image_xobject(doc, image);
        resources.set(
            "XObject",
            Object::Dictionary(Dictionary::from_iter(vec![(
                "MegaSigImg",
                Object::Reference(id),
            )])),
        );
        ops.push_str(&format!(
            "q {} 0 0 {} {} {} cm /MegaSigImg Do Q\n",
            num(fitted.w),
            num(fitted.h),
            num(fitted.x),
            num(fitted.y)
        ));
        text_x = slot.x + slot.w + 2.0 * pad;
    }
    // Texte : une taille commune, la plus grande qui tienne en largeur et en hauteur.
    let (text_w, text_h) = ((w - text_x - pad).max(1.0), (h - 2.0 * pad).max(1.0));
    // Une seule police pour toutes les lignes : la police Unicode dès qu'une ligne l'exige.
    let unicode = lines.iter().any(|l| fonts::needs_unicode(l));
    let size = font_size(lines, unicode, text_w, text_h);
    let mut writer = Type0Writer::new();
    let mut encoded = Vec::with_capacity(lines.len());
    for line in lines {
        encoded.push(if unicode {
            writer.encode(line)
        } else {
            // Repli prudent : un caractère non encodable (impossible ici) devient « ? ».
            hex_win_ansi(line).unwrap_or_else(|| "<3F>".to_string())
        });
    }
    let (font_key, font_id) = if unicode {
        ("MegaU1", writer.write(doc)?)
    } else {
        ("MegaF1", helvetica_font(doc))
    };
    resources.set(
        "Font",
        Object::Dictionary(Dictionary::from_iter(vec![(
            font_key,
            Object::Reference(font_id),
        )])),
    );
    // Coupé au cadre intérieur : un texte trop long ne déborde pas du tampon.
    ops.push_str(&format!(
        "q {} {} {} {} re W n BT /{font_key} {} Tf 0.1 0.1 0.35 rg\n",
        num(text_x),
        num(pad),
        num(text_w),
        num(text_h),
        num(size)
    ));
    for (i, hex) in encoded.iter().enumerate() {
        let baseline = h - pad - size * (0.9 + LINE_HEIGHT * i as f32);
        ops.push_str(&format!(
            "1 0 0 1 {} {} Tm {hex} Tj\n",
            num(text_x),
            num(baseline)
        ));
    }
    ops.push_str("ET Q\n");

    let mut dict = Dictionary::from_iter(vec![
        ("Type", Object::Name(b"XObject".to_vec())),
        ("Subtype", Object::Name(b"Form".to_vec())),
        (
            "BBox",
            Object::Array(vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Real(w),
                Object::Real(h),
            ]),
        ),
        ("Resources", Object::Dictionary(resources)),
    ]);
    if let Some([a, b, c, d]) = placement.rotation {
        dict.set(
            "Matrix",
            Object::Array(
                [a, b, c, d, 0.0, 0.0]
                    .iter()
                    .map(|v| Object::Real(v.round()))
                    .collect(),
            ),
        );
    }
    let mut stream = Stream::new(dict, ops.into_bytes());
    let _ = stream.compress();
    Ok(doc.add_object(stream))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Relecture du Lot 10 (I2) : dès qu'une ligne demande la police Unicode, **toutes** les
    /// lignes sont dessinées avec elle ; la taille doit donc être calculée avec ses largeurs.
    #[test]
    fn stamp_size_measures_every_line_in_the_drawing_font() {
        let lines = vec![
            "Signé numériquement par".to_string(),
            "Łukasz Kowalski".to_string(),
            "Date : 28/10/2026 13:22 UTC".to_string(),
        ];
        let size = font_size(&lines, true, 142.0, 60.0);
        let font = crate::fonts::unicode::UnicodeFont::embedded();
        for line in &lines {
            assert!(
                font.text_width(line, size) <= 142.0 + 0.01,
                "{line} à {size} pt"
            );
        }
    }

    #[test]
    fn lines_show_signer_date_reason_and_place() {
        let options = SignOptions {
            reason: Some(" Accord ".to_string()),
            location: Some(String::new()),
            ..SignOptions::default()
        };
        let lines = lines("Alice", 1_800_000_000, &options);
        assert_eq!(
            lines,
            vec![
                "Signé numériquement par",
                "Alice",
                "Date : 15/01/2027 08:00 UTC",
                "Motif : Accord"
            ]
        );
    }
}
