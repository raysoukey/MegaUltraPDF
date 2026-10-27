// SPDX-License-Identifier: GPL-3.0-or-later
//! Comparaison visuelle de deux pages : les deux sont rendues à la même échelle, les pixels
//! identiques sont estompés, les pixels différents peints en rouge, et l'on compte.
//!
//! \[Apprentissage\] **Itérer deux images à la fois** : `zip` sur des tranches de 4 octets
//! (`as_chunks::<4>()`) parcourt les pixels RGBA des deux rendus en parallèle, sans index
//! ni arithmétique de tampon ; une zone hors de l'une des deux pages est simplement comptée
//! comme différente.

use crate::document::Document;
use crate::error::{CoreError, CoreResult};
use crate::geometry::Rotation;
use crate::render::RenderedPage;

/// Écart maximal par canal (0-255) en dessous duquel deux pixels sont « identiques »
/// (anticrénelage, arrondis de rendu).
pub const DIFF_TOLERANCE: u8 = 24;
/// Échelle maximale d'une comparaison (4 = 288 dpi).
pub const MAX_COMPARE_SCALE: f32 = 4.0;

/// Résultat d'une comparaison de pages (l'image traverse la frontière en PNG, les compteurs
/// en JSON : voir l'acteur).
#[derive(Debug, Clone)]
pub struct PageDiff {
    /// Image des différences (union des deux pages, fond blanc).
    pub image: RenderedPage,
    /// Pixels jugés différents (hors de l'une des pages compris).
    pub differing_pixels: u64,
    /// Pixels de l'image des différences.
    pub total_pixels: u64,
}

/// Pixel RGBA de `page` en (x, y), ou `None` hors de l'image.
fn pixel_at(page: &RenderedPage, x: u32, y: u32) -> Option<[u8; 4]> {
    if x >= page.width_px || y >= page.height_px {
        return None;
    }
    let i = ((y * page.width_px + x) * 4) as usize;
    page.rgba.get(i..i + 4).map(|p| [p[0], p[1], p[2], p[3]])
}

/// Compare la page `page_a` de `a` à la page `page_b` de `b`, rendues à `scale`
/// (annotations comprises).
pub fn compare_pages(
    a: &Document,
    page_a: u16,
    b: &Document,
    page_b: u16,
    scale: f32,
) -> CoreResult<PageDiff> {
    if !(scale > 0.0 && scale.is_finite() && scale <= MAX_COMPARE_SCALE) {
        return Err(CoreError::InvalidOperation(format!(
            "l'échelle de comparaison doit être comprise entre 0 (exclu) et {MAX_COMPARE_SCALE}"
        )));
    }
    let left = a.render_page_with(page_a, scale, Rotation::None)?;
    let right = b.render_page_with(page_b, scale, Rotation::None)?;
    let width = left.width_px.max(right.width_px);
    let height = left.height_px.max(right.height_px);
    let total = u64::from(width) * u64::from(height);
    let mut rgba = Vec::with_capacity((total * 4) as usize);
    let mut differing = 0u64;
    for y in 0..height {
        for x in 0..width {
            let (la, lb) = (pixel_at(&left, x, y), pixel_at(&right, x, y));
            let same = match (la, lb) {
                (Some(p), Some(q)) => p
                    .iter()
                    .zip(q.iter())
                    .take(3)
                    .all(|(c, d)| c.abs_diff(*d) <= DIFF_TOLERANCE),
                _ => false,
            };
            if same {
                // Estompé : 30 % de la couleur, 70 % de blanc, en gris (moyenne des canaux).
                let p = la.unwrap_or([255, 255, 255, 255]);
                let avg = (u16::from(p[0]) + u16::from(p[1]) + u16::from(p[2])) / 3;
                let v = (avg * 3 / 10 + 255 * 7 / 10).min(255) as u8;
                rgba.extend_from_slice(&[v, v, v, 255]);
            } else {
                differing += 1;
                rgba.extend_from_slice(&[220, 30, 30, 255]);
            }
        }
    }
    Ok(PageDiff {
        image: RenderedPage {
            width_px: width,
            height_px: height,
            rgba,
        },
        differing_pixels: differing,
        total_pixels: total,
    })
}
