// SPDX-License-Identifier: GPL-3.0-or-later
//! Images posées sur une page (signature visuelle du Lot 4, insertion du Lot 5) : lecture
//! bornée d'un fichier image, ajustement dans une zone, placement sur la page.
//!
//! \[Apprentissage\] **Entrée non fiable** : le fichier image vient de l'utilisateur, mais
//! un PNG peut être piégé (dimensions énormes, données corrompues). On borne la taille du
//! fichier et des pixels **avant** de décoder, et le décodage faillible
//! (`image::load_from_memory`) est traduit en erreur métier plutôt qu'en panique.
//!
//! **Insertion par `lopdf`** (Lot 14, audit E9) : PDFium réencodait toute image en pixels
//! Flate (un JPEG de 15 Ko devenait 85 Ko) et lui ajoutait un masque `/SMask` opaque même
//! sans transparence. `place` écrit lui-même l'objet image : un JPEG entre **tel quel**
//! (filtre `/DCTDecode`, que tout lecteur PDF sait décoder), un PNG en pixels Flate avec un
//! masque seulement s'il a vraiment de la transparence.

use std::collections::BTreeSet;
use std::io::Cursor;

use image::DynamicImage;
use lopdf::{Dictionary, Document as LoDocument, Object, ObjectId, Stream};
use pdfium_render::prelude::{PdfMatrix, PdfPage, PdfPageObjectsCommon, PdfPoints};

use crate::content::raster::{decode_jpeg, multiply};
use crate::error::{CoreError, CoreResult};
use crate::geometry::{PageFrame, Rect};
use crate::overlay::{append_content, ensure_resources, free_resource_name, num};

/// Taille maximale d'un fichier image (octets).
pub const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
/// Dimension maximale (pixels) d'une image.
pub const MAX_IMAGE_PX: u32 = 8192;
/// Nombre maximal de pixels d'une image : une image non JPEG est écrite en échantillons
/// (comprimés, mais un bruit se comprime mal), et chaque instantané d'historique en garde une
/// copie.
pub const MAX_IMAGE_PIXELS: u64 = 16_000_000;

/// Préfixe du nom de ressource (`/XObject`) des images insérées : `MegaIm1`, `MegaIm2`…
const IMAGE_PREFIX: &str = "MegaIm";

fn invalid(msg: String) -> CoreError {
    CoreError::InvalidOperation(msg)
}

/// Une image prête à entrer dans le PDF, telle que [`load_for_insertion`] l'a validée.
///
/// Pourquoi deux formes : un JPEG est déjà comprimé dans un format que le PDF comprend
/// (`/DCTDecode`) ; le décoder puis le réécrire en pixels le rendrait plusieurs fois plus
/// lourd. Tout autre format (PNG) n'a pas d'équivalent PDF et passe par ses pixels.
pub(crate) enum ImageData {
    /// Un JPEG de base ou progressif, 8 bits, en niveaux de gris ou en RVB : ses octets
    /// d'origine (déjà décodés une fois pour s'assurer qu'ils sont lisibles).
    Jpeg {
        /// Le fichier JPEG tel quel.
        bytes: Vec<u8>,
        /// Largeur en pixels.
        width: u32,
        /// Hauteur en pixels.
        height: u32,
        /// Vrai pour une seule composante (`/DeviceGray`), faux pour trois (`/DeviceRGB`).
        gray: bool,
    },
    /// Toute autre image, décodée : écrite en RVB 8 bits Flate, plus un masque `/SMask` si
    /// un pixel est transparent.
    Rgba(DynamicImage),
}

impl ImageData {
    /// Dimensions en pixels `(largeur, hauteur)`.
    pub(crate) fn dimensions(&self) -> (u32, u32) {
        match self {
            ImageData::Jpeg { width, height, .. } => (*width, *height),
            ImageData::Rgba(image) => (image.width(), image.height()),
        }
    }
}

/// Lit le fichier `path` et vérifie ses bornes (taille du fichier, dimensions annoncées par
/// l'en-tête) **avant** tout décodage des pixels. Rend les octets et les dimensions.
fn read_bounded(path: &str) -> CoreResult<(Vec<u8>, u32, u32)> {
    let size = std::fs::metadata(path)?.len();
    if size > MAX_IMAGE_BYTES {
        return Err(invalid(format!(
            "image trop volumineuse ({} Mo, maximum 20 Mo)",
            size / (1024 * 1024)
        )));
    }
    let bytes = std::fs::read(path)?;
    // Les dimensions sont lues dans l'en-tête **avant** de décoder les pixels : un PNG de
    // quelques Ko peut annoncer des dizaines de milliers de pixels.
    let reader = image::ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|e| invalid(format!("image illisible : {e}")))?;
    let (w, h) = reader
        .into_dimensions()
        .map_err(|e| invalid(format!("image illisible : {e}")))?;
    if w > MAX_IMAGE_PX || h > MAX_IMAGE_PX {
        return Err(invalid(format!(
            "image trop grande ({w}×{h} px, maximum {MAX_IMAGE_PX})"
        )));
    }
    if u64::from(w) * u64::from(h) > MAX_IMAGE_PIXELS {
        return Err(invalid(format!(
            "image trop grande ({w}×{h} px : {} mégapixels, maximum 16)",
            u64::from(w) * u64::from(h) / 1_000_000
        )));
    }
    Ok((bytes, w, h))
}

/// Lit et décode un fichier image en vérifiant les bornes avant de décoder les pixels.
pub(crate) fn load_bounded(path: &str) -> CoreResult<DynamicImage> {
    let (bytes, _, _) = read_bounded(path)?;
    image::load_from_memory(&bytes).map_err(|e| invalid(format!("image illisible : {e}")))
}

/// Lit un fichier image pour l'insérer dans une page : un JPEG gris ou RVB garde ses octets
/// ([`ImageData::Jpeg`]), toute autre image est décodée ([`ImageData::Rgba`]).
///
/// Un JPEG est tout de même décodé une fois, avec des limites posées **avant** l'allocation
/// ([`decode_jpeg`], ADR-0017) : on n'insère pas un fichier tronqué ou mensonger que les
/// lecteurs afficheraient en morceaux. Un JPEG CMJN (ou d'un codage que `/DCTDecode` ne
/// garantit pas) suit le chemin des pixels, comme avant le Lot 14.
pub(crate) fn load_for_insertion(path: &str) -> CoreResult<ImageData> {
    let (bytes, width, height) = read_bounded(path)?;
    if let Some(components @ (1 | 3)) = jpeg_components(&bytes) {
        if decode_jpeg(&bytes, width, height).is_none() {
            return Err(invalid("image JPEG illisible".to_string()));
        }
        return Ok(ImageData::Jpeg {
            bytes,
            width,
            height,
            gray: components == 1,
        });
    }
    let image =
        image::load_from_memory(&bytes).map_err(|e| invalid(format!("image illisible : {e}")))?;
    Ok(ImageData::Rgba(image))
}

/// Nombre de composantes (1 gris, 3 RVB, 4 CMJN) d'un JPEG **8 bits, de base ou progressif**
/// (marqueurs SOF0, SOF1, SOF2), lu dans son en-tête ; `None` si `bytes` n'est pas un JPEG
/// ou si son codage (sans perte, hiérarchique, arithmétique, 12 bits) n'est pas garanti par
/// le filtre `/DCTDecode` des lecteurs PDF.
///
/// \[Apprentissage\] **Lire un format binaire** : un JPEG est une suite de segments
/// `FF <marqueur> <longueur sur 2 octets> <données>`. On saute de segment en segment jusqu'à
/// l'en-tête d'image (SOF). Chaque accès passe par `get`, qui rend `None` au lieu de
/// paniquer si le fichier est tronqué : `?` arrête alors la lecture proprement.
fn jpeg_components(bytes: &[u8]) -> Option<u8> {
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut i = 2;
    // Chaque tour avance d'au moins un octet : la boucle se termine toujours.
    while i < bytes.len() {
        if *bytes.get(i)? != 0xFF {
            return None;
        }
        let marker = *bytes.get(i + 1)?;
        match marker {
            // Octet de remplissage : le marqueur suit.
            0xFF => i += 1,
            // Marqueurs sans longueur (TEM, RST0 à RST7).
            0x01 | 0xD0..=0xD7 => i += 2,
            _ => {
                let length =
                    usize::from(u16::from_be_bytes([*bytes.get(i + 2)?, *bytes.get(i + 3)?]));
                if length < 2 {
                    return None;
                }
                match marker {
                    // Données du SOF : précision (1 octet), hauteur (2), largeur (2), composantes (1).
                    0xC0..=0xC2 => {
                        let precision = *bytes.get(i + 4)?;
                        let components = *bytes.get(i + 9)?;
                        return (precision == 8).then_some(components);
                    }
                    // Autres SOF (sans perte, hiérarchique, arithmétique), début des données
                    // (SOS) ou fin d'image (EOI) avant tout SOF : pas pour `/DCTDecode`.
                    0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF | 0xD9 | 0xDA => {
                        return None;
                    }
                    _ => i += 2 + length,
                }
            }
        }
    }
    None
}

/// Ajuste `(image_w, image_h)` dans `rect` en gardant les proportions, centré.
pub(crate) fn fit(image_w: f32, image_h: f32, rect: &Rect) -> Rect {
    let scale = (rect.w / image_w).min(rect.h / image_h);
    let w = image_w * scale;
    let h = image_h * scale;
    Rect {
        x: rect.x + (rect.w - w) / 2.0,
        y: rect.y + (rect.h - h) / 2.0,
        w,
        h,
    }
}

/// Dictionnaire d'une image 8 bits `w` × `h` dans l'espace de couleur `space`.
fn image_dict(w: u32, h: u32, space: &str) -> Dictionary {
    Dictionary::from_iter(vec![
        ("Type", Object::Name(b"XObject".to_vec())),
        ("Subtype", Object::Name(b"Image".to_vec())),
        ("Width", Object::Integer(i64::from(w))),
        ("Height", Object::Integer(i64::from(h))),
        ("ColorSpace", Object::Name(space.as_bytes().to_vec())),
        ("BitsPerComponent", Object::Integer(8)),
    ])
}

/// Écrit les pixels de `image` : RVB 8 bits comprimé (Flate) et, **seulement si un pixel
/// n'est pas opaque**, un masque `/SMask` en niveaux de gris. Rend la référence de l'image.
///
/// Pourquoi tester les pixels et pas seulement le type : un PNG RVBA est souvent entièrement
/// opaque, et un masque tout blanc ne ferait qu'alourdir le fichier (et gêner PDF/A-1, qui
/// interdit la transparence).
pub(crate) fn pixels_xobject(doc: &mut LoDocument, image: &DynamicImage) -> ObjectId {
    let (w, h) = (image.width(), image.height());
    let mut dict = image_dict(w, h, "DeviceRGB");
    if image.color().has_alpha() {
        let alpha: Vec<u8> = image.to_rgba8().pixels().map(|p| p.0[3]).collect();
        if alpha.iter().any(|&a| a < 255) {
            let mut mask = Stream::new(image_dict(w, h, "DeviceGray"), alpha);
            // `compress` ne fait rien si le flux comprimé serait plus gros.
            let _ = mask.compress();
            dict.set("SMask", Object::Reference(doc.add_object(mask)));
        }
    }
    // Les couleurs sont gardées telles quelles (non prémultipliées) : c'est ce qu'attend un
    // `/SMask` sans `/Matte` (ISO 32000-1 §11.6.5.3).
    let mut stream = Stream::new(dict, image.to_rgb8().into_raw());
    let _ = stream.compress();
    doc.add_object(stream)
}

/// Écrit l'objet image de `image` et rend sa référence.
fn image_xobject(doc: &mut LoDocument, image: &ImageData) -> ObjectId {
    match image {
        ImageData::Jpeg {
            bytes,
            width,
            height,
            gray,
        } => {
            let space = if *gray { "DeviceGray" } else { "DeviceRGB" };
            let mut dict = image_dict(*width, *height, space);
            dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
            doc.add_object(Stream::new(dict, bytes.clone()))
        }
        ImageData::Rgba(pixels) => pixels_xobject(doc, pixels),
    }
}

/// Pose `image` sur la page `page` de `doc` (passe `lopdf`), ajustée et centrée dans `rect`
/// (points, espace **affiché**, ADR-0010).
///
/// Une image se dessine dans le carré unité, que la matrice `cm` emmène sur la page. Cette
/// matrice enchaîne deux étapes : le carré unité vers le rectangle affiché, puis l'espace
/// affiché vers l'espace utilisateur (`frame.matrix_to_user_space`) ; sur une page tournée,
/// la seconde tourne l'image avec la page, qui reste donc droite à l'écran. Le contenu ajouté,
/// `q … cm /MegaImN Do Q`, suit le contenu existant (enfermé dans `q … Q`), et l'image entre
/// dans des ressources **propres à la page** sous un nom libre.
pub(crate) fn place(
    doc: &mut LoDocument,
    page: ObjectId,
    frame: &PageFrame,
    rect: Rect,
    image: &ImageData,
) -> CoreResult<()> {
    let (w, h) = image.dimensions();
    let placed = fit(w as f32, h as f32, &rect);
    let id = image_xobject(doc, image);
    let name = free_resource_name(doc, &[page], b"XObject", IMAGE_PREFIX);
    ensure_resources(doc, page, &[("XObject", name.as_str(), id)])?;
    let unit_to_displayed = [placed.w, 0.0, 0.0, placed.h, placed.x, placed.y];
    let m = multiply(&unit_to_displayed, &frame.matrix_to_user_space());
    let content = format!(
        "q\n{} {} {} {} {} {} cm\n/{name} Do\nQ\n",
        num(m[0]),
        num(m[1]),
        num(m[2]),
        num(m[3]),
        num(m[4]),
        num(m[5])
    );
    // Une image insérée est du vrai contenu, pas une décoration : pas d'artefact.
    append_content(
        doc,
        page,
        &mut BTreeSet::new(),
        None,
        content.into_bytes(),
        None,
    )
}

/// Pose `image` dans `placed` (espace **affiché**) sur la page par PDFium, **jeton déjà pris
/// par l'appelant**. Sur une page tournée, la matrice du repère emmène l'image (tournée) dans
/// l'espace utilisateur : elle reste droite à l'écran.
///
/// Sert encore à l'image d'un champ de signature (`forms::signature`) ; l'insertion d'image
/// passe par [`place`] (Lot 14).
pub(crate) fn place_with_pdfium(
    pdf_page: &mut PdfPage<'_>,
    frame: &PageFrame,
    image: &DynamicImage,
    placed: &Rect,
) -> CoreResult<()> {
    let mut obj = pdf_page.objects_mut().create_image_object(
        PdfPoints::new(placed.x),
        PdfPoints::new(placed.y),
        image,
        Some(PdfPoints::new(placed.w)),
        Some(PdfPoints::new(placed.h)),
    )?;
    if !frame.is_identity() {
        let m = frame.matrix_to_user_space();
        obj.apply_matrix(PdfMatrix::new(m[0], m[1], m[2], m[3], m[4], m[5]))?;
    }
    pdf_page.regenerate_content()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// JPEG de `image` (un tampon typé : encoder un `DynamicImage` passerait par le RVB).
    fn jpeg_of<I>(image: &I) -> Vec<u8>
    where
        I: image::GenericImageView,
        <I as image::GenericImageView>::Pixel: image::PixelWithColorType,
    {
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut out)
            .encode_image(image)
            .unwrap();
        out
    }

    #[test]
    fn jpeg_components_reads_the_frame_header() {
        let rgb = jpeg_of(&image::RgbImage::new(4, 3));
        assert_eq!(jpeg_components(&rgb), Some(3));
        let gray = jpeg_of(&image::GrayImage::new(4, 3));
        assert_eq!(jpeg_components(&gray), Some(1));
    }

    #[test]
    fn jpeg_components_refuses_other_or_truncated_data() {
        assert_eq!(jpeg_components(b"\x89PNG\r\n\x1a\n"), None);
        let rgb = jpeg_of(&image::RgbImage::new(4, 3));
        // Tronqué juste après le début : aucun SOF lisible, sans paniquer.
        assert_eq!(jpeg_components(&rgb[..6]), None);
        // SOF3 (sans perte) : pas pour `/DCTDecode`.
        assert_eq!(
            jpeg_components(&[0xFF, 0xD8, 0xFF, 0xC3, 0x00, 0x0B, 8, 0, 3, 0, 4, 1]),
            None
        );
        // Longueur de segment impossible.
        assert_eq!(jpeg_components(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x01]), None);
        // 12 bits : refusé.
        assert_eq!(
            jpeg_components(&[0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x0B, 12, 0, 3, 0, 4, 3]),
            None
        );
    }

    #[test]
    fn opaque_pixels_get_no_mask() {
        let mut doc = LoDocument::with_version("1.7");
        let opaque = DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            3,
            2,
            image::Rgba([1, 2, 3, 255]),
        ));
        let id = pixels_xobject(&mut doc, &opaque);
        let stream = doc.get_object(id).unwrap().as_stream().unwrap();
        assert!(!stream.dict.has(b"SMask"));
        let mut see_through = image::RgbaImage::from_pixel(3, 2, image::Rgba([1, 2, 3, 255]));
        see_through.put_pixel(0, 0, image::Rgba([1, 2, 3, 10]));
        let id = pixels_xobject(&mut doc, &DynamicImage::ImageRgba8(see_through));
        let stream = doc.get_object(id).unwrap().as_stream().unwrap();
        assert!(stream.dict.has(b"SMask"));
    }
}
