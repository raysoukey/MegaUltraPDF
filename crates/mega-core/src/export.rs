// SPDX-License-Identifier: GPL-3.0-or-later
//! Export d'un document en images (PNG ou JPEG, résolution choisie) et en texte brut.
//!
//! \[Apprentissage\] **`std::io::Write` et `Cursor`** : les encodeurs d'`image` écrivent dans
//! « n'importe quoi qui implémente `Write` ». Un `Vec<u8>` (via `Cursor`) en est un : on encode
//! en mémoire, puis on écrit le fichier d'un bloc avec `write_atomically`. Le trait `Write`
//! est ce qui rend l'encodeur indifférent à la destination (fichier, mémoire, réseau).

use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::ImageEncoder;
use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::edit::write_atomically;
use crate::error::{CoreError, CoreResult};
use crate::geometry::Rotation;

/// Nombre maximal de pixels d'une image exportée (une page A4 à 600 dpi fait ≈ 35 Mpx).
pub const MAX_EXPORT_PIXELS: u64 = 50_000_000;
/// Résolution minimale (points par pouce) d'un export.
pub const MIN_DPI: u16 = 36;
/// Résolution maximale d'un export.
pub const MAX_DPI: u16 = 600;

/// Format d'image de sortie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImageFormat {
    /// PNG sans perte.
    Png,
    /// JPEG avec perte, qualité 10-100.
    Jpeg {
        /// Qualité de compression.
        quality: u8,
    },
}

impl ImageFormat {
    /// Extension de fichier (sans point).
    pub fn extension(self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg { .. } => "jpg",
        }
    }
}

/// Ce qu'il faut exporter et comment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageExport {
    /// Pages (index 0) ; toutes si `None`.
    pub pages: Option<Vec<u16>>,
    /// Format.
    pub format: ImageFormat,
    /// Résolution en points par pouce ([`MIN_DPI`] à [`MAX_DPI`]).
    pub dpi: u16,
    /// Dessiner les annotations et les champs de formulaire.
    pub with_annotations: bool,
}

fn invalid(msg: String) -> CoreError {
    CoreError::InvalidOperation(msg)
}

impl Document {
    /// Exporte des pages en images `<prefix>-<N>.<ext>` (N = numéro de page, à partir de 1)
    /// dans `dir` (créé si besoin). Sans `force`, un fichier déjà présent fait tout refuser
    /// **avant** la première écriture : on ne laisse jamais un export à moitié fait par-dessus
    /// des fichiers existants. Retourne les chemins écrits, dans l'ordre.
    pub fn export_images(
        &self,
        spec: &ImageExport,
        dir: &Path,
        prefix: &str,
        force: bool,
    ) -> CoreResult<Vec<PathBuf>> {
        if !(MIN_DPI..=MAX_DPI).contains(&spec.dpi) {
            return Err(invalid(format!(
                "la résolution doit être comprise entre {MIN_DPI} et {MAX_DPI} dpi"
            )));
        }
        if let ImageFormat::Jpeg { quality } = spec.format
            && !(10..=100).contains(&quality)
        {
            return Err(invalid(
                "la qualité JPEG doit être comprise entre 10 et 100".to_string(),
            ));
        }
        let prefix = prefix.trim();
        if prefix.is_empty() || prefix.contains(['/', '\\']) {
            return Err(invalid("préfixe de fichier invalide".to_string()));
        }
        let pages = self.resolve_pages(spec.pages.as_deref())?;
        let scale = f32::from(spec.dpi) / 72.0;
        // Budget vérifié page par page avant de rendre quoi que ce soit.
        for &p in &pages {
            let size = self.page_size(p)?;
            let px = (f64::from(size.width_pt) * f64::from(scale)).round()
                * (f64::from(size.height_pt) * f64::from(scale)).round();
            if px > MAX_EXPORT_PIXELS as f64 {
                return Err(invalid(format!(
                    "la page {} ferait {:.0} mégapixels à {} dpi (maximum {})",
                    p + 1,
                    px / 1e6,
                    spec.dpi,
                    MAX_EXPORT_PIXELS / 1_000_000
                )));
            }
        }
        let paths: Vec<PathBuf> = pages
            .iter()
            .map(|p| dir.join(format!("{prefix}-{}.{}", p + 1, spec.format.extension())))
            .collect();
        if !force && let Some(existing) = paths.iter().find(|p| p.exists()) {
            return Err(invalid(format!(
                "le fichier {} existe déjà (utilisez « écraser »)",
                existing.display()
            )));
        }
        std::fs::create_dir_all(dir)?;
        for (&page, path) in pages.iter().zip(&paths) {
            let rendered =
                self.render_page_opts(page, scale, Rotation::None, spec.with_annotations)?;
            let bytes = encode(&rendered, spec.format)?;
            write_atomically(path, &bytes)?;
        }
        Ok(paths)
    }

    /// Texte de toutes les pages demandées (toutes si `None`), dans l'ordre, avec ou sans
    /// ligne de séparation `===== Page N =====`.
    pub fn export_text(&self, pages: Option<&[u16]>, separators: bool) -> CoreResult<String> {
        let pages = self.resolve_pages(pages)?;
        let mut out = String::new();
        for p in pages {
            if separators {
                out.push_str(&format!("===== Page {} =====\n", p + 1));
            }
            out.push_str(self.page_text(p)?.trim_end());
            out.push('\n');
        }
        Ok(out)
    }

    /// Pages demandées validées (toutes si `None`) : uniques, triées, existantes.
    fn resolve_pages(&self, pages: Option<&[u16]>) -> CoreResult<Vec<u16>> {
        let count = self.page_count();
        match pages {
            None => Ok((0..count).collect()),
            Some(list) => {
                if list.is_empty() {
                    return Err(invalid("aucune page sélectionnée".to_string()));
                }
                let set: std::collections::BTreeSet<u16> = list.iter().copied().collect();
                for &p in &set {
                    if p >= count {
                        return Err(CoreError::PageOutOfRange { index: p, count });
                    }
                }
                Ok(set.into_iter().collect())
            }
        }
    }
}

/// Encode une page rendue dans le format demandé, en mémoire.
fn encode(page: &crate::render::RenderedPage, format: ImageFormat) -> CoreResult<Vec<u8>> {
    match format {
        ImageFormat::Png => page.to_png(),
        ImageFormat::Jpeg { quality } => {
            // Le JPEG n'a pas de canal alpha : on aplatit sur blanc.
            let rgb: Vec<u8> = page
                .rgba
                .chunks_exact(4)
                .flat_map(|px| {
                    let a = u16::from(px[3]);
                    let blend = |c: u8| ((u16::from(c) * a + 255 * (255 - a)) / 255) as u8;
                    [blend(px[0]), blend(px[1]), blend(px[2])]
                })
                .collect();
            let mut out = Cursor::new(Vec::new());
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality)
                .write_image(
                    &rgb,
                    page.width_px,
                    page.height_px,
                    image::ExtendedColorType::Rgb8,
                )
                .map_err(|e| CoreError::Pdfium(format!("encodage JPEG : {e}")))?;
            Ok(out.into_inner())
        }
    }
}
