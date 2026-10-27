// SPDX-License-Identifier: GPL-3.0-or-later
//! Optimisation conservatrice d'un document par `lopdf` : objets inutilisés élagués, flux non
//! compressés passés en Flate, images RGB / gris 8 bits recomprimées en JPEG et réduites.
//!
//! « Conservatrice » : on ne touche qu'aux images dont on comprend complètement la forme
//! (pas de masque, pas de `/Decode`, espace de couleur simple), on ne remplace une image que
//! si le résultat est plus petit, et si le fichier entier n'est pas plus petit à la fin, le
//! document n'est pas modifié du tout.
//!
//! \[Apprentissage\] **`Option` comme filtre d'éligibilité** : `eligible()` renvoie `None` dès
//! qu'une condition manque, et l'appelant enchaîne avec `let Some(..) = … else { continue }`.
//! Chaque `?` à l'intérieur d'une fonction qui renvoie `Option` lit « sinon, pas éligible ».

use std::io::Cursor;

use image::{DynamicImage, ImageEncoder};
use lopdf::{Dictionary, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::error::{CoreError, CoreResult};

/// Nombre maximal de pixels d'une image que l'on accepte de décoder.
const MAX_DECODE_PIXELS: u64 = 50_000_000;

/// Réglages de l'optimisation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptimizeOptions {
    /// Qualité JPEG (10-100) des images recomprimées ; `None` = ne pas recomprimer.
    pub image_quality: Option<u8>,
    /// Dimension maximale (pixels) d'une image, au-delà de laquelle elle est réduite
    /// (256-8192) ; `None` = ne pas réduire.
    pub max_image_px: Option<u32>,
    /// Compresser en Flate les flux non compressés (contenu de page, polices…).
    pub compress_streams: bool,
}

impl Default for OptimizeOptions {
    fn default() -> Self {
        OptimizeOptions {
            image_quality: Some(75),
            max_image_px: None,
            compress_streams: true,
        }
    }
}

/// Ce que l'optimisation a fait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OptimizeReport {
    /// Taille du document avant, en octets.
    pub bytes_before: u64,
    /// Taille après (égale à `bytes_before` si rien n'a été gagné : document inchangé).
    pub bytes_after: u64,
    /// Images recomprimées ou réduites.
    pub images_recompressed: u16,
    /// Objets inutilisés retirés.
    pub objects_removed: u32,
}

fn invalid(msg: &str) -> CoreError {
    CoreError::InvalidOperation(msg.to_string())
}

/// Espace de couleur d'une image éligible.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Space {
    Rgb,
    Gray,
}

/// Filtre d'une image éligible.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    None,
    Flate,
    Dct,
}

/// Ce qu'il faut savoir d'une image pour la décoder.
struct Candidate {
    width: u32,
    height: u32,
    space: Space,
    filter: Filter,
}

fn name_of(obj: &Object) -> Option<&[u8]> {
    match obj {
        Object::Name(n) => Some(n.as_slice()),
        Object::Array(items) if items.len() == 1 => items[0].as_name().ok(),
        _ => None,
    }
}

/// Décide si un flux est une image que l'on sait recomprimer sans risque.
fn eligible(dict: &Dictionary) -> Option<Candidate> {
    if dict.get(b"Subtype").ok().and_then(|s| s.as_name().ok())? != b"Image" {
        return None;
    }
    for forbidden in [b"SMask".as_slice(), b"Mask", b"Decode", b"ImageMask", b"OC"] {
        if dict.has(forbidden) {
            return None;
        }
    }
    if dict.get(b"BitsPerComponent").ok()?.as_i64().ok()? != 8 {
        return None;
    }
    let space = match name_of(dict.get(b"ColorSpace").ok()?)? {
        b"DeviceRGB" => Space::Rgb,
        b"DeviceGray" => Space::Gray,
        _ => return None,
    };
    let filter = match dict.get(b"Filter") {
        Err(_) => Filter::None,
        Ok(f) => match name_of(f)? {
            b"FlateDecode" => Filter::Flate,
            b"DCTDecode" => Filter::Dct,
            _ => return None,
        },
    };
    if dict.has(b"DecodeParms") {
        // Flate : un prédicteur PNG change l'ordre des octets ; DCT : `/ColorTransform`
        // change l'interprétation des couleurs. Dans les deux cas : hors périmètre.
        return None;
    }
    let width = u32::try_from(dict.get(b"Width").ok()?.as_i64().ok()?).ok()?;
    let height = u32::try_from(dict.get(b"Height").ok()?.as_i64().ok()?).ok()?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_DECODE_PIXELS {
        return None;
    }
    Some(Candidate {
        width,
        height,
        space,
        filter,
    })
}

/// Décode les pixels d'une image éligible.
fn decode(stream: &Stream, c: &Candidate) -> Option<DynamicImage> {
    let channels: usize = match c.space {
        Space::Rgb => 3,
        Space::Gray => 1,
    };
    let expected = c.width as usize * c.height as usize * channels;
    match c.filter {
        Filter::Dct => {
            image::load_from_memory_with_format(&stream.content, image::ImageFormat::Jpeg).ok()
        }
        Filter::None | Filter::Flate => {
            let raw = if c.filter == Filter::None {
                stream.content.clone()
            } else {
                stream.decompressed_content_with_limit(expected + 1).ok()?
            };
            if raw.len() != expected {
                return None;
            }
            match c.space {
                Space::Rgb => {
                    image::RgbImage::from_raw(c.width, c.height, raw).map(DynamicImage::ImageRgb8)
                }
                Space::Gray => {
                    image::GrayImage::from_raw(c.width, c.height, raw).map(DynamicImage::ImageLuma8)
                }
            }
        }
    }
}

/// Réduit si nécessaire puis encode en JPEG ; `None` si rien n'est à gagner.
fn recompress(stream: &Stream, c: &Candidate, opts: &OptimizeOptions) -> Option<Stream> {
    let mut img = decode(stream, c)?;
    let mut resized = false;
    if let Some(max) = opts.max_image_px
        && img.width().max(img.height()) > max
    {
        let scale = f64::from(max) / f64::from(img.width().max(img.height()));
        let w = ((f64::from(img.width()) * scale).round() as u32).max(1);
        let h = ((f64::from(img.height()) * scale).round() as u32).max(1);
        img = img.resize_exact(w, h, image::imageops::FilterType::Triangle);
        resized = true;
    }
    let quality = opts.image_quality.unwrap_or(85);
    let mut out = Cursor::new(Vec::new());
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
    match c.space {
        Space::Rgb => encoder
            .write_image(
                img.to_rgb8().as_raw(),
                img.width(),
                img.height(),
                image::ExtendedColorType::Rgb8,
            )
            .ok()?,
        Space::Gray => encoder
            .write_image(
                img.to_luma8().as_raw(),
                img.width(),
                img.height(),
                image::ExtendedColorType::L8,
            )
            .ok()?,
    }
    let encoded = out.into_inner();
    if !resized && encoded.len() >= stream.content.len() {
        return None;
    }
    let mut dict = stream.dict.clone();
    dict.set("Width", i64::from(img.width()));
    dict.set("Height", i64::from(img.height()));
    dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
    dict.remove(b"DecodeParms");
    Some(Stream::new(dict, encoded).with_compression(false))
}

impl Document {
    /// Ce que [`optimize`](Self::optimize) ferait, sans toucher au document : l'optimisation
    /// tourne sur une copie. L'interface montre ce rapport avant de proposer d'appliquer.
    pub fn optimize_preview(&self, opts: &OptimizeOptions) -> CoreResult<OptimizeReport> {
        let mut copy = Document::from_bytes(self.to_bytes()?, self.password())?;
        copy.optimize(opts)
    }

    /// Optimise le document ; le rapport dit ce qui a été fait. Si le résultat n'est pas plus
    /// petit, le document n'est pas modifié (`bytes_after == bytes_before`).
    pub fn optimize(&mut self, opts: &OptimizeOptions) -> CoreResult<OptimizeReport> {
        self.mark_changed();
        if let Some(q) = opts.image_quality
            && !(10..=100).contains(&q)
        {
            return Err(invalid(
                "la qualité JPEG doit être comprise entre 10 et 100",
            ));
        }
        if let Some(px) = opts.max_image_px
            && !(256..=8192).contains(&px)
        {
            return Err(invalid(
                "la dimension maximale des images doit être comprise entre 256 et 8192 pixels",
            ));
        }
        let bytes = self.to_bytes()?;
        let mut doc = lopdf::Document::load_mem(&bytes).map_err(|e| {
            CoreError::InvalidOperation(format!("structure du PDF illisible : {e}"))
        })?;
        if doc.is_encrypted() || doc.was_encrypted() {
            return Err(invalid(
                "optimisation d'un PDF chiffré : non prise en charge dans cette version",
            ));
        }
        let mut report = OptimizeReport {
            bytes_before: bytes.len() as u64,
            bytes_after: bytes.len() as u64,
            ..OptimizeReport::default()
        };
        let before = doc.objects.len();
        doc.prune_objects();
        report.objects_removed =
            u32::try_from(before.saturating_sub(doc.objects.len())).unwrap_or(u32::MAX);
        // Les masques (`/SMask`, `/Mask` en flux) sont des images grises que rien ne distingue
        // d'une candidate ; les réencoder en JPEG (alpha avec perte) ou les réduire seuls (alors
        // que `/Matte` exige la taille de l'image parente) abîmerait l'image qu'ils masquent.
        let masks: std::collections::BTreeSet<ObjectId> = doc
            .objects
            .values()
            .filter_map(|o| o.as_stream().ok())
            .flat_map(|s| {
                [b"SMask".as_slice(), b"Mask"]
                    .into_iter()
                    .filter_map(|k| s.dict.get(k).ok().and_then(|m| m.as_reference().ok()))
            })
            .collect();
        let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
        for id in ids {
            if masks.contains(&id) {
                continue;
            }
            let Some(Object::Stream(stream)) = doc.objects.get(&id) else {
                continue;
            };
            if let Some(candidate) = eligible(&stream.dict) {
                let wanted = opts.image_quality.is_some() || opts.max_image_px.is_some();
                if wanted && let Some(new_stream) = recompress(stream, &candidate, opts) {
                    doc.objects.insert(id, Object::Stream(new_stream));
                    report.images_recompressed = report.images_recompressed.saturating_add(1);
                }
            } else if opts.compress_streams
                && stream.dict.get(b"Filter").is_err()
                // XMP : PDF/A-1 interdit `/Filter` sur le flux de métadonnées, et des outils le
                // lisent en clair dans le fichier.
                && stream.dict.get(b"Type").and_then(|t| t.as_name()).ok() != Some(b"Metadata".as_slice())
                && stream.allows_compression
                && let Some(Object::Stream(stream)) = doc.objects.get_mut(&id)
            {
                // Échec de compression = flux laissé tel quel.
                let _ = stream.compress();
            }
        }
        doc.renumber_objects();
        let pages = doc.get_pages().len();
        let mut out = Vec::new();
        doc.save_to(&mut out)?;
        if out.len() >= bytes.len() || !crate::cleanup::rewrite_is_sane(pages, &out) {
            return Ok(OptimizeReport {
                images_recompressed: 0,
                objects_removed: 0,
                ..report
            });
        }
        report.bytes_after = out.len() as u64;
        self.reload_from_bytes(out)?;
        Ok(report)
    }
}
