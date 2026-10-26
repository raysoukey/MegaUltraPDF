// SPDX-License-Identifier: GPL-3.0-or-later
//! Types géométriques partagés avec le frontend (rectangles en points PDF, rotation).

use pdfium_render::prelude::{PdfPageRenderRotation, PdfRect};
use serde::{Deserialize, Serialize};

/// Rectangle en points PDF, **origine en bas à gauche** de la page (convention PDF).
///
/// Le frontend le convertit en pixels écran (origine en haut à gauche) avec l'échelle et
/// la rotation courantes : voir `app/src/lib/logic/geometry.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    /// Abscisse du bord gauche.
    pub x: f32,
    /// Ordonnée du bord **bas**.
    pub y: f32,
    /// Largeur.
    pub w: f32,
    /// Hauteur.
    pub h: f32,
}

impl Rect {
    /// Convertit un rectangle PDFium (bottom/left/top/right) en `Rect`.
    pub(crate) fn from_pdf(r: PdfRect) -> Rect {
        Rect {
            x: r.left().value,
            y: r.bottom().value,
            w: (r.right().value - r.left().value).max(0.0),
            h: (r.top().value - r.bottom().value).max(0.0),
        }
    }
}

/// Rotation d'affichage d'une page, dans le sens horaire.
///
/// Sérialisée en chaîne (`"none"`, `"cw90"`, …) pour le frontend et le protocole `mega://`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rotation {
    /// Aucune rotation.
    #[default]
    None,
    /// Quart de tour horaire.
    Cw90,
    /// Demi-tour.
    Cw180,
    /// Trois quarts de tour horaire.
    Cw270,
}

impl Rotation {
    /// Angle en degrés (0, 90, 180, 270).
    pub fn degrees(self) -> u16 {
        match self {
            Rotation::None => 0,
            Rotation::Cw90 => 90,
            Rotation::Cw180 => 180,
            Rotation::Cw270 => 270,
        }
    }

    /// Construit une rotation à partir d'un angle ; tout autre angle vaut `None`.
    pub fn from_degrees(deg: u16) -> Rotation {
        match deg % 360 {
            90 => Rotation::Cw90,
            180 => Rotation::Cw180,
            270 => Rotation::Cw270,
            _ => Rotation::None,
        }
    }

    /// Rotation suivante dans le sens horaire.
    pub fn next_cw(self) -> Rotation {
        Rotation::from_degrees(self.degrees() + 90)
    }

    /// Vrai si la rotation échange largeur et hauteur.
    pub fn swaps_axes(self) -> bool {
        matches!(self, Rotation::Cw90 | Rotation::Cw270)
    }

    pub(crate) fn to_pdfium(self) -> PdfPageRenderRotation {
        match self {
            Rotation::None => PdfPageRenderRotation::None,
            Rotation::Cw90 => PdfPageRenderRotation::Degrees90,
            Rotation::Cw180 => PdfPageRenderRotation::Degrees180,
            Rotation::Cw270 => PdfPageRenderRotation::Degrees270,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_cycles_clockwise() {
        assert_eq!(Rotation::None.next_cw(), Rotation::Cw90);
        assert_eq!(Rotation::Cw270.next_cw(), Rotation::None);
        assert!(Rotation::Cw90.swaps_axes() && !Rotation::Cw180.swaps_axes());
        assert_eq!(Rotation::from_degrees(450), Rotation::Cw90);
    }
}
