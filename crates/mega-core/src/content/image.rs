// SPDX-License-Identifier: GPL-3.0-or-later
//! Images posées sur une page (signature visuelle du Lot 4, insertion du Lot 5) : lecture
//! bornée d'un fichier image, ajustement dans une zone, placement sur la page.
//!
//! \[Apprentissage\] **Entrée non fiable** : le fichier image vient de l'utilisateur, mais
//! un PNG peut être piégé (dimensions énormes, données corrompues). On borne la taille du
//! fichier et des pixels **avant** de décoder, et le décodage faillible
//! (`image::load_from_memory`) est traduit en erreur métier plutôt qu'en panique.

use std::io::Cursor;

use image::DynamicImage;
use pdfium_render::prelude::{PdfMatrix, PdfPage, PdfPageObjectsCommon, PdfPoints};

use crate::error::{CoreError, CoreResult};
use crate::geometry::{PageFrame, Rect};

/// Taille maximale d'un fichier image (octets).
pub const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
/// Dimension maximale (pixels) d'une image.
pub const MAX_IMAGE_PX: u32 = 8192;
/// Nombre maximal de pixels d'une image : PDFium écrit les échantillons **non compressés**
/// (plus un masque alpha), et chaque instantané d'historique en garde une copie.
pub const MAX_IMAGE_PIXELS: u64 = 16_000_000;

fn invalid(msg: String) -> CoreError {
    CoreError::InvalidOperation(msg)
}

/// Lit et décode un fichier image en vérifiant les bornes avant de décoder les pixels.
pub(crate) fn load_bounded(path: &str) -> CoreResult<DynamicImage> {
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
    image::load_from_memory(&bytes).map_err(|e| invalid(format!("image illisible : {e}")))
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

/// Pose `image` dans `placed` (espace **affiché**) sur la page, **jeton déjà pris par
/// l'appelant**. Sur une page tournée, la matrice du repère emmène l'image (tournée) dans
/// l'espace utilisateur : elle reste droite à l'écran.
pub(crate) fn place(
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
