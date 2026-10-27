// SPDX-License-Identifier: GPL-3.0-or-later
//! Types géométriques partagés avec le frontend (rectangles en points PDF, rotation).

use pdfium_render::prelude::{PdfPage, PdfPageRenderRotation, PdfRect};
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

/// Repère d'une page : ce qu'il faut pour passer de l'espace **affiché** (la page telle que
/// PDFium la rend, `/Rotate` appliqué, origine en bas à gauche de la zone visible) à l'espace
/// **utilisateur** du fichier (celui des `/Rect`, des `/QuadPoints` et du contenu).
///
/// Toute l'API publique de `mega-core` parle en espace affiché : c'est ce que l'interface
/// montre et ce que l'utilisateur désigne. La conversion n'a lieu qu'au moment d'écrire ou de
/// lire le fichier (ADR-0010).
///
/// \[Apprentissage\] Une rotation de page n'est qu'un **changement de repère** : on soustrait
/// l'origine de la boîte, on échange ou on renverse les axes, et on refait l'inverse au retour.
/// Les quatre cas sont écrits explicitement plutôt qu'avec une matrice générique : c'est plus
/// long, mais chaque ligne se vérifie à la main (voir les tests en bas du fichier).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageFrame {
    /// `/Rotate` de la page : 0, 90, 180 ou 270 (quart de tours horaires à l'affichage).
    pub rotate: u16,
    /// Zone visible de la page en espace utilisateur (CropBox ∩ MediaBox).
    pub bbox: Rect,
}

impl PageFrame {
    /// Dimensions de la page affichée `(largeur, hauteur)` : celles de la zone visible, axes
    /// échangés pour un quart de tour.
    pub fn displayed_size(&self) -> (f32, f32) {
        if self.rotate % 180 == 90 {
            (self.bbox.h, self.bbox.w)
        } else {
            (self.bbox.w, self.bbox.h)
        }
    }

    /// Vrai si les deux espaces coïncident (pas de rotation, boîte à l'origine) : la
    /// conversion est alors inutile et on évite de toucher aux objets.
    pub fn is_identity(&self) -> bool {
        self.rotate.is_multiple_of(360) && self.bbox.x == 0.0 && self.bbox.y == 0.0
    }

    /// Un point de l'espace utilisateur vers l'espace affiché.
    pub fn point_from_user_space(&self, p: [f32; 2]) -> [f32; 2] {
        let (u, v) = (p[0] - self.bbox.x, p[1] - self.bbox.y);
        let (w, h) = (self.bbox.w, self.bbox.h);
        match self.rotate % 360 {
            90 => [v, w - u],
            180 => [w - u, h - v],
            270 => [h - v, u],
            _ => [u, v],
        }
    }

    /// Un point de l'espace affiché vers l'espace utilisateur.
    pub fn point_to_user_space(&self, p: [f32; 2]) -> [f32; 2] {
        let (w, h) = (self.bbox.w, self.bbox.h);
        let (u, v) = match self.rotate % 360 {
            90 => (w - p[1], p[0]),
            180 => (w - p[0], h - p[1]),
            270 => (p[1], h - p[0]),
            _ => (p[0], p[1]),
        };
        [u + self.bbox.x, v + self.bbox.y]
    }

    /// Un rectangle de l'espace utilisateur vers l'espace affiché.
    pub fn from_user_space(&self, r: Rect) -> Rect {
        Self::rect_from_corners(
            self.point_from_user_space([r.x, r.y]),
            self.point_from_user_space([r.x + r.w, r.y + r.h]),
        )
    }

    /// Un rectangle de l'espace affiché vers l'espace utilisateur.
    pub fn to_user_space(&self, r: Rect) -> Rect {
        Self::rect_from_corners(
            self.point_to_user_space([r.x, r.y]),
            self.point_to_user_space([r.x + r.w, r.y + r.h]),
        )
    }

    /// Matrice affine `[a b c d e f]` qui envoie l'espace affiché dans l'espace utilisateur
    /// (`x' = a·x + c·y + e`, `y' = b·x + d·y + f`), à appliquer à un objet dessiné en
    /// coordonnées affichées (texte, image) pour qu'il apparaisse droit à l'écran.
    pub fn matrix_to_user_space(&self) -> [f32; 6] {
        let Rect { x, y, w, h } = self.bbox;
        match self.rotate % 360 {
            90 => [0.0, 1.0, -1.0, 0.0, x + w, y],
            180 => [-1.0, 0.0, 0.0, -1.0, x + w, y + h],
            270 => [0.0, -1.0, 1.0, 0.0, x, y + h],
            _ => [1.0, 0.0, 0.0, 1.0, x, y],
        }
    }

    /// Rectangle normalisé à partir de deux coins opposés (dans n'importe quel ordre).
    fn rect_from_corners(a: [f32; 2], b: [f32; 2]) -> Rect {
        Rect {
            x: a[0].min(b[0]),
            y: a[1].min(b[1]),
            w: (a[0] - b[0]).abs(),
            h: (a[1] - b[1]).abs(),
        }
    }
}

/// Repère d'une page PDFium, **jeton déjà pris par l'appelant**.
///
/// La zone visible vient de `FPDF_GetPageBoundingBox` (CropBox ∩ MediaBox, en espace
/// utilisateur) ; à défaut, de la MediaBox ; en dernier recours, de la taille rendue avec
/// les axes remis dans l'ordre utilisateur.
pub(crate) fn frame_of(page: &PdfPage<'_>) -> PageFrame {
    let rotate = page
        .rotation()
        .map(Rotation::from_pdfium)
        .unwrap_or_default()
        .degrees();
    let bbox = page
        .boundaries()
        .bounding()
        .or_else(|_| page.boundaries().media())
        .map(|b| Rect::from_pdf(b.bounds))
        .unwrap_or_else(|_| {
            let (w, h) = (page.width().value, page.height().value);
            let (w, h) = if rotate % 180 == 90 { (h, w) } else { (w, h) };
            Rect {
                x: 0.0,
                y: 0.0,
                w,
                h,
            }
        });
    PageFrame { rotate, bbox }
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

    pub(crate) fn from_pdfium(r: PdfPageRenderRotation) -> Rotation {
        match r {
            PdfPageRenderRotation::None => Rotation::None,
            PdfPageRenderRotation::Degrees90 => Rotation::Cw90,
            PdfPageRenderRotation::Degrees180 => Rotation::Cw180,
            PdfPageRenderRotation::Degrees270 => Rotation::Cw270,
        }
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

    fn frame(rotate: u16) -> PageFrame {
        PageFrame {
            rotate,
            bbox: Rect {
                x: 10.0,
                y: 20.0,
                w: 400.0,
                h: 600.0,
            },
        }
    }

    fn same(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    #[test]
    fn page_frame_maps_the_user_origin_corner_for_each_rotation() {
        // Le coin bas-gauche utilisateur (10, 20) se retrouve, à l'affichage…
        let origin = [10.0, 20.0];
        assert!(same(frame(0).point_from_user_space(origin), [0.0, 0.0]));
        // … en haut à gauche après un quart de tour horaire (page affichée 600 × 400),
        assert!(same(frame(90).point_from_user_space(origin), [0.0, 400.0]));
        // … en haut à droite après un demi-tour,
        assert!(same(
            frame(180).point_from_user_space(origin),
            [400.0, 600.0]
        ));
        // … en bas à droite après trois quarts de tour.
        assert!(same(frame(270).point_from_user_space(origin), [600.0, 0.0]));
        assert_eq!(frame(90).displayed_size(), (600.0, 400.0));
        assert_eq!(frame(180).displayed_size(), (400.0, 600.0));
    }

    #[test]
    fn page_frame_round_trips_points_rects_and_matches_its_matrix() {
        for rotate in [0, 90, 180, 270] {
            let f = frame(rotate);
            let p = [123.0, 45.0];
            let u = f.point_to_user_space(p);
            assert!(same(f.point_from_user_space(u), p), "rotation {rotate}");
            let m = f.matrix_to_user_space();
            let via_matrix = [
                m[0] * p[0] + m[2] * p[1] + m[4],
                m[1] * p[0] + m[3] * p[1] + m[5],
            ];
            assert!(same(via_matrix, u), "matrice {rotate}");
            let r = Rect {
                x: 50.0,
                y: 30.0,
                w: 100.0,
                h: 40.0,
            };
            let back = f.from_user_space(f.to_user_space(r));
            assert!(same([back.x, back.y], [r.x, r.y]) && same([back.w, back.h], [r.w, r.h]));
            let user = f.to_user_space(r);
            assert!(user.w > 0.0 && user.h > 0.0);
            if f.rotate % 180 == 90 {
                assert!(same([user.w, user.h], [r.h, r.w]), "axes échangés");
            }
        }
        assert!(!frame(0).is_identity(), "origine décalée : pas l'identité");
    }

    #[test]
    fn rotation_cycles_clockwise() {
        assert_eq!(Rotation::None.next_cw(), Rotation::Cw90);
        assert_eq!(Rotation::Cw270.next_cw(), Rotation::None);
        assert!(Rotation::Cw90.swaps_axes() && !Rotation::Cw180.swaps_axes());
        assert_eq!(Rotation::from_degrees(450), Rotation::Cw90);
    }
}
