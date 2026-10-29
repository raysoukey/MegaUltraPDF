// SPDX-License-Identifier: GPL-3.0-or-later
//! `cargo xtask brand` : produit les images de l'application à partir des originaux de
//! `assets/brand/` (Lot 12, identité visuelle).
//!
//! - `actions.png` est une planche de 12 icônes (4 colonnes, 3 rangées), chacune avec sa
//!   légende écrite dessous. On veut les icônes **sans** légende : dans chaque colonne, les
//!   bandes de pixels opaques alternent icône, légende, icône, légende…, séparées par des
//!   lignes transparentes. On garde une bande sur deux, on la recadre au plus près, on la
//!   centre sur un carré transparent et on la réduit en 64 × 64 (`app/static/icons/`).
//! - `logo.png` est réduit pour l'écran d'accueil et « À propos » (`app/static/logo.png`).
//! - `banner-fr.png` est réduit pour le README (`docs/images/banniere.jpg`).
//!
//! L'icône de l'application (`app/src-tauri/icons/`) se régénère à part, avec l'outil
//! officiel : `pnpm --dir app tauri icon ../assets/brand/icon.png` (le chemin part de `app/`),
//! puis on supprime `android/`, `ios/` et `64x64.png`, que l'application n'utilise pas.
//!
//! \[Apprentissage\] **Traiter une image en Rust** : le crate `image` charge un PNG dans un
//! `RgbaImage`, une grille de pixels `[r, g, b, a]` qu'on lit avec `get_pixel(x, y)`. Les
//! opérations courantes (recadrer, coller, redimensionner) sont dans `image::imageops`.
//! Voir `docs/apprentissage/lot-12.md`.

use std::ops::Range;
use std::path::Path;

use anyhow::{Context, bail};
use image::RgbaImage;
use image::imageops::{self, FilterType};

/// Noms des 12 icônes, dans l'ordre de la planche (de gauche à droite, puis de haut en bas).
/// Le frontend les reprend dans `app/src/lib/logic/icons.ts`.
pub const ICON_NAMES: [&str; 12] = [
    "open", "save", "export", "merge", "split", "rotate", "annotate", "sign", "ocr", "search",
    "protect", "compress",
];
/// Nombre de colonnes de la planche.
const COLUMNS: u32 = 4;
/// Nombre de rangées de la planche.
const ROWS: usize = 3;
/// Côté des icônes produites, en pixels : affichées en 24 px (18 px dans les menus), elles
/// restent nettes sur un écran à forte densité (jusqu'à 2,7×).
const ICON_SIZE: u32 = 64;
/// Opacité (sur 255) au-delà de laquelle un pixel compte comme dessiné : ignore le léger
/// halo laissé par le détourage.
const ALPHA: u8 = 32;
/// Une bande se termine après au moins `MIN_GAP` lignes transparentes.
const MIN_GAP: usize = 6;
/// Une bande plus courte que `MIN_LEN` lignes est une poussière, pas un dessin.
const MIN_LEN: usize = 8;
/// Marge transparente autour de l'icône, en pixels de la planche (≈ 2 px une fois réduite).
const MARGIN: u32 = 8;
/// Largeurs du logo et de la bannière produits.
const LOGO_WIDTH: u32 = 480;
const BANNER_WIDTH: u32 = 1280;

/// Rectangle de pixels : coin haut gauche, largeur, hauteur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    /// Colonne du coin haut gauche.
    pub x: u32,
    /// Ligne du coin haut gauche.
    pub y: u32,
    /// Largeur en pixels.
    pub w: u32,
    /// Hauteur en pixels.
    pub h: u32,
}

/// Les suites de `true` de `flags`, comme intervalles `(début, fin)` inclus.
///
/// Deux suites séparées par moins de `min_gap` valeurs `false` n'en font qu'une (un trou dans
/// un dessin n'est pas la fin de l'icône) ; une suite de moins de `min_len` valeurs est
/// ignorée (poussière).
pub fn runs(flags: &[bool], min_gap: usize, min_len: usize) -> Vec<(usize, usize)> {
    // D'abord toutes les suites brutes, puis on recolle celles que sépare un petit trou.
    let mut raw: Vec<(usize, usize)> = Vec::new();
    let mut start = None;
    for (i, &drawn) in flags.iter().enumerate() {
        match (drawn, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                raw.push((s, i - 1));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        raw.push((s, flags.len() - 1));
    }
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (s, e) in raw {
        match merged.last_mut() {
            // `s - last.1 - 1` lignes vides séparent les deux suites.
            Some(last) if s - last.1 - 1 < min_gap => last.1 = e,
            _ => merged.push((s, e)),
        }
    }
    merged.retain(|&(s, e)| e - s + 1 >= min_len);
    merged
}

/// Le plus petit rectangle qui contient tous les pixels dessinés (opacité > [`ALPHA`]) de
/// `img` dans la zone `xs` × `ys`, ou `None` si la zone est vide.
pub fn bounding_box(img: &RgbaImage, xs: Range<u32>, ys: Range<u32>) -> Option<Rect> {
    let mut found: Option<(u32, u32, u32, u32)> = None;
    for y in ys {
        for x in xs.clone() {
            if img.get_pixel(x, y)[3] > ALPHA {
                found = Some(match found {
                    None => (x, y, x, y),
                    Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
                });
            }
        }
    }
    found.map(|(x0, y0, x1, y1)| Rect {
        x: x0,
        y: y0,
        w: x1 - x0 + 1,
        h: y1 - y0 + 1,
    })
}

/// Le carré qui accueille un dessin de `w` × `h` avec une marge `margin` tout autour :
/// `(côté, décalage x, décalage y)`, le dessin étant centré.
pub fn square_canvas(w: u32, h: u32, margin: u32) -> (u32, u32, u32) {
    let side = w.max(h) + 2 * margin;
    (side, (side - w) / 2, (side - h) / 2)
}

/// Les 12 icônes de la planche, sans légende, en carrés de [`ICON_SIZE`] pixels, dans l'ordre
/// de [`ICON_NAMES`].
pub fn cut_icons(sheet: &RgbaImage) -> anyhow::Result<Vec<RgbaImage>> {
    let (width, height) = sheet.dimensions();
    let column_width = width / COLUMNS;
    // Pour chaque colonne : sa zone horizontale et les bandes d'icônes (une sur deux).
    let mut columns = Vec::new();
    for c in 0..COLUMNS {
        let xs = c * column_width..(c + 1) * column_width;
        let drawn_rows: Vec<bool> = (0..height)
            .map(|y| xs.clone().any(|x| sheet.get_pixel(x, y)[3] > ALPHA))
            .collect();
        let bands = runs(&drawn_rows, MIN_GAP, MIN_LEN);
        if bands.len() != ROWS * 2 {
            bail!(
                "colonne {c} de la planche : {} bandes au lieu de {} (icône, légende)",
                bands.len(),
                ROWS * 2
            );
        }
        let icon_bands: Vec<(usize, usize)> = bands.into_iter().step_by(2).collect();
        columns.push((xs, icon_bands));
    }
    let mut icons = Vec::new();
    for row in 0..ROWS {
        for (xs, icon_bands) in &columns {
            let (top, bottom) = icon_bands[row];
            let ys = u32::try_from(top)?..u32::try_from(bottom + 1)?;
            let rect =
                bounding_box(sheet, xs.clone(), ys).context("bande d'icône sans pixel dessiné")?;
            let drawing = imageops::crop_imm(sheet, rect.x, rect.y, rect.w, rect.h).to_image();
            let (side, dx, dy) = square_canvas(rect.w, rect.h, MARGIN);
            let mut canvas = RgbaImage::new(side, side);
            imageops::overlay(&mut canvas, &drawing, i64::from(dx), i64::from(dy));
            icons.push(imageops::resize(
                &canvas,
                ICON_SIZE,
                ICON_SIZE,
                FilterType::Lanczos3,
            ));
        }
    }
    Ok(icons)
}

/// Produit toutes les images dérivées à partir de `assets/brand/`.
pub fn generate(root: &Path) -> anyhow::Result<()> {
    let brand = root.join("assets/brand");
    let open = |name: &str| {
        let path = brand.join(name);
        image::open(&path).with_context(|| format!("lecture de {}", path.display()))
    };

    let icons_dir = root.join("app/static/icons");
    std::fs::create_dir_all(&icons_dir)?;
    let sheet = open("actions.png")?.to_rgba8();
    for (name, icon) in ICON_NAMES.iter().zip(cut_icons(&sheet)?) {
        let path = icons_dir.join(format!("{name}.png"));
        icon.save(&path)
            .with_context(|| format!("écriture de {}", path.display()))?;
    }
    println!("{} icônes dans {}", ICON_NAMES.len(), icons_dir.display());

    // `resize` garde les proportions : la hauteur donnée n'est qu'un plafond.
    let logo = open("logo.png")?.resize(LOGO_WIDTH, u32::MAX, FilterType::Lanczos3);
    let logo_path = root.join("app/static/logo.png");
    logo.save(&logo_path)
        .with_context(|| format!("écriture de {}", logo_path.display()))?;
    println!("logo : {}", logo_path.display());

    // La bannière est une image « photo » : en JPEG, elle pèse dix fois moins qu'en PNG.
    let banner = open("banner-fr.png")?
        .resize(BANNER_WIDTH, u32::MAX, FilterType::Lanczos3)
        .to_rgb8();
    let images_dir = root.join("docs/images");
    std::fs::create_dir_all(&images_dir)?;
    let banner_path = images_dir.join("banniere.jpg");
    let file = std::fs::File::create(&banner_path)
        .with_context(|| format!("écriture de {}", banner_path.display()))?;
    let mut encoder =
        image::codecs::jpeg::JpegEncoder::new_with_quality(std::io::BufWriter::new(file), 85);
    encoder.encode_image(&banner)?;
    println!("bannière : {}", banner_path.display());

    println!(
        "Icône de l'application (à lancer à part) : pnpm --dir app tauri icon ../assets/brand/icon.png, \
         puis supprimer app/src-tauri/icons/android, ios et 64x64.png"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    const OPAQUE: Rgba<u8> = Rgba([200, 60, 20, 255]);

    #[test]
    fn runs_separate_the_icon_from_its_caption() {
        // 3 lignes vides, une icône de 10 lignes, 7 lignes vides, une légende de 9 lignes.
        let mut flags = vec![false; 3];
        flags.extend([true; 10]);
        flags.extend([false; 7]);
        flags.extend([true; 9]);
        assert_eq!(runs(&flags, 6, 4), vec![(3, 12), (20, 28)]);
    }

    #[test]
    fn runs_bridge_small_holes_and_drop_specks() {
        // Un trou de 2 lignes dans l'icône ne la coupe pas ; une poussière de 2 lignes est ignorée.
        let mut flags = vec![true; 5];
        flags.extend([false; 2]);
        flags.extend([true; 5]);
        flags.extend([false; 8]);
        flags.extend([true; 2]);
        assert_eq!(runs(&flags, 6, 4), vec![(0, 11)]);
        assert_eq!(runs(&[false; 5], 6, 4), vec![]);
        // Une suite qui touche la fin compte aussi.
        assert_eq!(runs(&[false, true, true, true, true], 6, 4), vec![(1, 4)]);
    }

    #[test]
    fn bounding_box_finds_drawn_pixels_in_the_zone() {
        let mut img = RgbaImage::new(20, 20);
        for y in 3..8 {
            for x in 5..10 {
                img.put_pixel(x, y, OPAQUE);
            }
        }
        // Un pixel à peine visible (halo) ne compte pas.
        img.put_pixel(15, 15, Rgba([0, 0, 0, 10]));
        assert_eq!(
            bounding_box(&img, 0..20, 0..20),
            Some(Rect {
                x: 5,
                y: 3,
                w: 5,
                h: 5
            })
        );
        // Hors de la zone demandée, rien.
        assert_eq!(bounding_box(&img, 12..20, 0..20), None);
    }

    #[test]
    fn square_canvas_centres_the_drawing() {
        assert_eq!(square_canvas(10, 6, 2), (14, 2, 4));
        assert_eq!(square_canvas(6, 10, 0), (10, 2, 0));
    }

    /// Une planche synthétique de 4 × 3 cases : une couleur d'icône propre à chaque case et,
    /// dessous, une « légende » toujours de la même couleur. Chaque icône produite doit avoir
    /// la couleur de **sa** case (ordre de gauche à droite, puis de haut en bas) et **aucun**
    /// pixel de légende (relecture du Lot 12 : le test sur la vraie planche ne pouvait pas le
    /// voir).
    #[test]
    fn cut_icons_keeps_the_sheet_order_and_drops_the_captions() {
        const CAPTION: Rgba<u8> = Rgba([0, 0, 255, 255]);
        let cell = |i: u32| Rgba([10 + 20 * i as u8, 200, 30, 255]);
        let mut sheet = RgbaImage::new(4 * 60, 3 * 60);
        for row in 0..3 {
            for col in 0..4 {
                let (x0, y0) = (col * 60, row * 60);
                // Icône : 30 × 30 ; 10 lignes vides ; légende : 30 × 8.
                for y in 5..35 {
                    for x in 15..45 {
                        sheet.put_pixel(x0 + x, y0 + y, cell(row * 4 + col));
                    }
                }
                for y in 45..53 {
                    for x in 15..45 {
                        sheet.put_pixel(x0 + x, y0 + y, CAPTION);
                    }
                }
            }
        }
        let icons = cut_icons(&sheet).unwrap();
        assert_eq!(icons.len(), 12);
        for (i, icon) in icons.iter().enumerate() {
            let centre = icon.get_pixel(ICON_SIZE / 2, ICON_SIZE / 2);
            // Le filtre de réduction peut décaler une composante d'une unité.
            let expected = cell(i as u32);
            let close = (0..4).all(|c| centre[c].abs_diff(expected[c]) <= 2);
            assert!(close, "icône {i} : mauvaise case ({centre:?})");
            let blue = icon.pixels().any(|p| p[2] > 128 && p[3] > ALPHA);
            assert!(!blue, "icône {i} : un pixel de légende est resté");
        }
    }

    /// La vraie planche du projet donne 12 icônes carrées. La garde contre une légende restée
    /// est le contrôle « 6 bandes par colonne » de `cut_icons` : une légende collée à son icône
    /// ferait 5 bandes, et `cut_icons` refuserait la planche.
    #[test]
    fn the_project_sheet_gives_twelve_icons() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("assets/brand/actions.png");
        let sheet = image::open(&path).unwrap().to_rgba8();
        let icons = cut_icons(&sheet).unwrap();
        assert_eq!(icons.len(), ICON_NAMES.len());
        for icon in &icons {
            assert_eq!(icon.dimensions(), (ICON_SIZE, ICON_SIZE));
        }
    }
}
