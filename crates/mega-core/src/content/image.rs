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
use crate::fonts::unicode::compressed;
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
        /// Vrai si les trois composantes sont codées en RVB et non en YCbCr (voir
        /// [`JpegHeader::rgb_coded`]) : le lecteur ne doit pas les convertir.
        rgb_coded: bool,
    },
    /// Toute autre image, décodée : écrite en pixels 8 bits Flate (gris ou RVB), plus un
    /// masque `/SMask` si un pixel est transparent.
    Pixels(DynamicImage),
}

impl ImageData {
    /// Dimensions en pixels `(largeur, hauteur)`.
    pub(crate) fn dimensions(&self) -> (u32, u32) {
        match self {
            ImageData::Jpeg { width, height, .. } => (*width, *height),
            ImageData::Pixels(image) => (image.width(), image.height()),
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
/// ([`ImageData::Jpeg`]), toute autre image est décodée ([`ImageData::Pixels`]).
///
/// Un JPEG est tout de même décodé une fois, avec des limites posées **avant** l'allocation
/// ([`decode_jpeg`], ADR-0017) : on n'insère pas un fichier que le décodeur refuse (tables
/// manquantes, en-tête mensonger). Le décodeur est indulgent pour les **données** compressées
/// (un fichier tronqué se décode, complété de gris) : les lecteurs PDF le sont autant. Un
/// JPEG CMJN (ou d'un codage que `/DCTDecode` ne garantit pas) suit le chemin des pixels,
/// comme avant le Lot 14.
pub(crate) fn load_for_insertion(path: &str) -> CoreResult<ImageData> {
    let (bytes, width, height) = read_bounded(path)?;
    if let Some(header) = dct_header(&bytes) {
        if decode_jpeg(&bytes, width, height).is_none() {
            return Err(invalid("image JPEG illisible".to_string()));
        }
        return Ok(ImageData::Jpeg {
            bytes,
            width,
            height,
            gray: header.components == 1,
            rgb_coded: header.rgb_coded,
        });
    }
    let image =
        image::load_from_memory(&bytes).map_err(|e| invalid(format!("image illisible : {e}")))?;
    Ok(ImageData::Pixels(image))
}

/// Ce que dit l'en-tête d'un JPEG, lu par [`jpeg_header`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct JpegHeader {
    /// Nombre de composantes : 1 gris, 3 couleur, 4 CMJN.
    components: u8,
    /// Vrai pour trois composantes nommées `R`, `G`, `B` (identifiants du SOF) **sans**
    /// marqueur JFIF (APP0) ni Adobe (APP14) : la convention de libjpeg pour un JPEG codé en
    /// RVB. Sans le dire (`/ColorTransform 0`), un lecteur PDF appliquerait la conversion
    /// YCbCr → RVB par défaut à trois composantes (ISO 32000-1, tableau 13) et fausserait les
    /// couleurs. Un marqueur JFIF impose YCbCr quels que soient les identifiants, et un marqueur
    /// Adobe décide à la place : dans les deux cas le décodeur n'écoute pas les identifiants,
    /// nous non plus.
    rgb_coded: bool,
}

/// L'en-tête d'un JPEG que `/DCTDecode` garantit et que nous gardons tel quel : gris ou
/// couleur (une ou trois composantes). `None` pour le reste (CMJN compris), qui suit le
/// chemin des pixels.
fn dct_header(bytes: &[u8]) -> Option<JpegHeader> {
    jpeg_header(bytes).filter(|h| matches!(h.components, 1 | 3))
}

/// L'en-tête (composantes, codage RVB) d'un JPEG **8 bits, de base ou progressif**
/// (marqueurs SOF0, SOF1, SOF2) ; `None` si `bytes` n'est pas un JPEG ou si son codage (sans
/// perte, hiérarchique, arithmétique, 12 bits) n'est pas garanti par le filtre `/DCTDecode`
/// des lecteurs PDF.
///
/// \[Apprentissage\] **Lire un format binaire** : un JPEG est une suite de segments
/// `FF <marqueur> <longueur sur 2 octets> <données>`. On saute de segment en segment jusqu'à
/// l'en-tête d'image (SOF). Chaque accès passe par `get`, qui rend `None` au lieu de
/// paniquer si le fichier est tronqué : `?` arrête alors la lecture proprement.
fn jpeg_header(bytes: &[u8]) -> Option<JpegHeader> {
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    // Un marqueur JFIF (APP0) ou Adobe (APP14) vu avant le SOF : les identifiants de
    // composantes ne comptent plus.
    let mut ids_overridden = false;
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
                    // Données du SOF : précision (1 octet), hauteur (2), largeur (2), nombre de
                    // composantes (1), puis trois octets par composante, dont l'identifiant
                    // en tête (`i + 10`, `i + 13`, `i + 16`).
                    0xC0..=0xC2 => {
                        let precision = *bytes.get(i + 4)?;
                        let components = *bytes.get(i + 9)?;
                        let ids = [bytes.get(i + 10), bytes.get(i + 13), bytes.get(i + 16)];
                        let rgb_coded = components == 3
                            && !ids_overridden
                            && ids == [Some(&b'R'), Some(&b'G'), Some(&b'B')];
                        return (precision == 8).then_some(JpegHeader {
                            components,
                            rgb_coded,
                        });
                    }
                    // APP0 : `JFIF` suivi d'un octet nul en tête de ses données.
                    0xE0 => {
                        ids_overridden |= bytes.get(i + 4..i + 9) == Some(b"JFIF\0".as_slice());
                        i += 2 + length;
                    }
                    // APP14 : `Adobe` en tête de ses données.
                    0xEE => {
                        ids_overridden |= bytes.get(i + 4..i + 9) == Some(b"Adobe".as_slice());
                        i += 2 + length;
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

/// Écrit les pixels de `image` : 8 bits comprimés (Flate), en niveaux de gris pour une
/// image sans couleur (trois fois moins d'octets), en RVB sinon ; et, **seulement si un
/// pixel n'est pas opaque**, un masque `/SMask` en niveaux de gris. Rend la référence de
/// l'image.
///
/// Pourquoi tester les pixels et pas seulement le type : un PNG RVBA est souvent entièrement
/// opaque, et un masque tout blanc ne ferait qu'alourdir le fichier (et gêner PDF/A-1, qui
/// interdit la transparence).
pub(crate) fn pixels_xobject(doc: &mut LoDocument, image: &DynamicImage) -> ObjectId {
    let (w, h) = (image.width(), image.height());
    let (space, samples) = if image.color().has_color() {
        ("DeviceRGB", image.to_rgb8().into_raw())
    } else {
        ("DeviceGray", image.to_luma8().into_raw())
    };
    let mut dict = image_dict(w, h, space);
    if let Some(alpha) = alpha_channel(image)
        && alpha.iter().any(|&a| a < 255)
    {
        let mask = compressed(image_dict(w, h, "DeviceGray"), alpha);
        dict.set("SMask", Object::Reference(doc.add_object(mask)));
    }
    // Les couleurs sont gardées telles quelles (non prémultipliées) : c'est ce qu'attend un
    // `/SMask` sans `/Matte` (ISO 32000-1 §11.6.5.3).
    doc.add_object(compressed(dict, samples))
}

/// La transparence de `image`, un octet par pixel ; `None` sans canal alpha.
///
/// \[Apprentissage\] `match` sur les variantes de `DynamicImage` : pour les formes 8 bits
/// courantes, on lit l'alpha **dans** le tampon existant, sans le recopier en RVBA entier
/// (`to_rgba8` allouerait quatre octets par pixel pour n'en garder qu'un).
fn alpha_channel(image: &DynamicImage) -> Option<Vec<u8>> {
    if !image.color().has_alpha() {
        return None;
    }
    Some(match image {
        DynamicImage::ImageRgba8(buffer) => buffer.pixels().map(|p| p.0[3]).collect(),
        DynamicImage::ImageLumaA8(buffer) => buffer.pixels().map(|p| p.0[1]).collect(),
        // 16 bits ou flottants (rares) : conversion en 8 bits.
        other => other.to_rgba8().pixels().map(|p| p.0[3]).collect(),
    })
}

/// Écrit l'objet image de `image` et rend sa référence. Prend `image` **par valeur** : les
/// octets d'un JPEG (jusqu'à 20 Mo) entrent dans le flux sans copie.
fn image_xobject(doc: &mut LoDocument, image: ImageData) -> ObjectId {
    match image {
        ImageData::Jpeg {
            bytes,
            width,
            height,
            gray,
            rgb_coded,
        } => {
            let space = if gray { "DeviceGray" } else { "DeviceRGB" };
            let mut dict = image_dict(width, height, space);
            dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
            if rgb_coded {
                dict.set(
                    "DecodeParms",
                    Object::Dictionary(Dictionary::from_iter(vec![(
                        "ColorTransform",
                        Object::Integer(0),
                    )])),
                );
            }
            doc.add_object(Stream::new(dict, bytes))
        }
        ImageData::Pixels(pixels) => pixels_xobject(doc, &pixels),
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
    image: ImageData,
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

    /// Nombre de composantes lu dans l'en-tête.
    fn jpeg_components(bytes: &[u8]) -> Option<u8> {
        jpeg_header(bytes).map(|h| h.components)
    }

    #[test]
    fn jpeg_components_reads_the_frame_header() {
        let rgb = jpeg_of(&image::RgbImage::new(4, 3));
        assert_eq!(
            jpeg_header(&rgb),
            Some(JpegHeader {
                components: 3,
                rgb_coded: false
            })
        );
        let gray = jpeg_of(&image::GrayImage::new(4, 3));
        assert_eq!(jpeg_components(&gray), Some(1));
    }

    /// Un SOF0 8 bits de 4 × 3 pixels dont les composantes portent les identifiants `ids`,
    /// précédé de `prefix` (segments facultatifs).
    fn sof(prefix: &[u8], ids: &[u8]) -> Vec<u8> {
        let mut out = vec![0xFF, 0xD8];
        out.extend_from_slice(prefix);
        let length = 8 + 3 * ids.len() as u16;
        out.extend_from_slice(&[0xFF, 0xC0]);
        out.extend_from_slice(&length.to_be_bytes());
        out.extend_from_slice(&[8, 0, 3, 0, 4, ids.len() as u8]);
        for id in ids {
            out.extend_from_slice(&[*id, 0x11, 0]);
        }
        out.extend_from_slice(&[0xFF, 0xD9]);
        out
    }

    /// Revue de la partie D : identifiants `R`, `G`, `B` sans marqueur Adobe → codé en RVB ;
    /// un marqueur Adobe (APP14) décide à la place, et un marqueur JFIF (APP0) impose YCbCr
    /// (règle de libjpeg) ; un autre segment APP ne change rien.
    #[test]
    fn rgb_component_ids_mean_rgb_coding() {
        let header = |bytes: &[u8]| jpeg_header(bytes).unwrap();
        assert!(header(&sof(&[], b"RGB")).rgb_coded);
        assert!(!header(&sof(&[], &[1, 2, 3])).rgb_coded);
        let mut app14 = vec![0xFF, 0xEE, 0x00, 0x0E];
        app14.extend_from_slice(b"Adobe");
        app14.extend_from_slice(&[0, 100, 0, 0, 0, 0, 0]);
        assert!(!header(&sof(&app14, b"RGB")).rgb_coded);
        // Un marqueur JFIF impose YCbCr : les identifiants ne comptent plus.
        let mut app0 = vec![0xFF, 0xE0, 0x00, 0x07];
        app0.extend_from_slice(b"JFIF\0");
        assert!(!header(&sof(&app0, b"RGB")).rgb_coded);
        // Un autre segment APP (ici un APP1 quelconque) ne change rien.
        let app1 = vec![0xFF, 0xE1, 0x00, 0x04, 0x41, 0x42];
        assert!(header(&sof(&app1, b"RGB")).rgb_coded);
    }

    /// Revue de la partie D : un JPEG CMJN (quatre composantes) n'est pas gardé tel quel, il
    /// suit le chemin des pixels.
    #[test]
    fn a_cmyk_jpeg_takes_the_pixel_path() {
        let cmyk = sof(&[], &[1, 2, 3, 4]);
        assert_eq!(jpeg_components(&cmyk), Some(4));
        assert_eq!(dct_header(&cmyk), None);
        assert!(dct_header(&sof(&[], &[1, 2, 3])).is_some());
        assert!(dct_header(&sof(&[], &[1])).is_some());
    }

    /// Un JPEG codé en RVB est écrit avec `/ColorTransform 0` ; un JPEG YCbCr sans paramètre.
    #[test]
    fn rgb_coded_jpeg_gets_color_transform_zero() {
        for rgb_coded in [true, false] {
            let mut doc = LoDocument::with_version("1.7");
            let id = image_xobject(
                &mut doc,
                ImageData::Jpeg {
                    bytes: sof(&[], b"RGB"),
                    width: 4,
                    height: 3,
                    gray: false,
                    rgb_coded,
                },
            );
            let stream = doc.get_object(id).unwrap().as_stream().unwrap();
            let transform = stream
                .dict
                .get(b"DecodeParms")
                .and_then(Object::as_dict)
                .and_then(|d| d.get(b"ColorTransform"))
                .and_then(Object::as_i64)
                .ok();
            assert_eq!(transform, rgb_coded.then_some(0), "RVB : {rgb_coded}");
        }
    }

    /// Revue de la partie D : une image sans couleur est écrite en `/DeviceGray`, un octet
    /// par pixel (trois fois moins qu'en RVB), avec son masque s'il est transparent.
    #[test]
    fn grey_pixels_are_written_as_device_gray() {
        let mut doc = LoDocument::with_version("1.7");
        let gray = DynamicImage::ImageLuma8(image::GrayImage::from_pixel(3, 2, image::Luma([90])));
        let id = pixels_xobject(&mut doc, &gray);
        let stream = doc.get_object(id).unwrap().as_stream().unwrap();
        assert_eq!(
            stream.dict.get(b"ColorSpace").unwrap().as_name().unwrap(),
            b"DeviceGray"
        );
        assert_eq!(crate::crypt::stream_bytes(stream).unwrap(), vec![90; 6]);
        assert!(!stream.dict.has(b"SMask"));
        let mut see_through = image::GrayAlphaImage::from_pixel(3, 2, image::LumaA([90, 255]));
        see_through.put_pixel(2, 1, image::LumaA([90, 0]));
        let id = pixels_xobject(&mut doc, &DynamicImage::ImageLumaA8(see_through));
        let stream = doc.get_object(id).unwrap().as_stream().unwrap();
        assert_eq!(
            stream.dict.get(b"ColorSpace").unwrap().as_name().unwrap(),
            b"DeviceGray"
        );
        let mask = stream.dict.get(b"SMask").unwrap().as_reference().unwrap();
        let mask = doc.get_object(mask).unwrap().as_stream().unwrap();
        assert_eq!(
            crate::crypt::stream_bytes(mask).unwrap(),
            vec![255, 255, 255, 255, 255, 0]
        );
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
