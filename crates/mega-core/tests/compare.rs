// SPDX-License-Identifier: GPL-3.0-or-later
//! Comparaison visuelle de deux pages : image des différences et compteur.

use mega_core::compare::compare_pages;
use mega_core::testing::{blank_pdf, sample_pdf};
use mega_core::{Color, CoreError, Document, FontFamily, TextStyle};

fn pixel(img: &mega_core::RenderedPage, x: u32, y: u32) -> (u8, u8, u8) {
    let i = ((y * img.width_px + x) * 4) as usize;
    (img.rgba[i], img.rgba[i + 1], img.rgba[i + 2])
}

#[test]
fn identical_pages_have_no_difference() {
    let a = Document::from_bytes(sample_pdf(1, "a"), None).unwrap();
    let b = Document::from_bytes(sample_pdf(1, "b"), None).unwrap();
    let diff = compare_pages(&a, 0, &b, 0, 1.0).unwrap();
    assert_eq!(diff.differing_pixels, 0);
    assert_eq!(diff.total_pixels, 595 * 842);
    assert_eq!((diff.image.width_px, diff.image.height_px), (595, 842));
    // Le fond identique est rendu estompé (gris clair), pas rouge.
    let (r, g, b) = pixel(&diff.image, 300, 400);
    assert!(r > 200 && r == g && g == b, "{:?}", (r, g, b));
}

#[test]
fn modified_page_is_flagged_where_it_changed() {
    let a = Document::from_bytes(sample_pdf(1, "a"), None).unwrap();
    let mut b = Document::from_bytes(sample_pdf(1, "a"), None).unwrap();
    b.insert_text(
        0,
        [100.0, 500.0],
        "Nouveau",
        &TextStyle {
            family: FontFamily::Helvetica,
            bold: true,
            italic: false,
            size: 36.0,
            color: Color { r: 0, g: 0, b: 0 },
        },
    )
    .unwrap();
    let diff = compare_pages(&a, 0, &b, 0, 1.0).unwrap();
    assert!(diff.differing_pixels > 200, "{}", diff.differing_pixels);
    assert!(diff.differing_pixels < diff.total_pixels / 10);
    // Le texte ajouté est vers (100..250, 470..500) en PDF : des pixels rouges s'y trouvent.
    let mut red = 0;
    for y in (842 - 500)..(842 - 465) {
        for x in 100..260 {
            let (r, g, b) = pixel(&diff.image, x, y);
            if r > 200 && g < 80 && b < 80 {
                red += 1;
            }
        }
    }
    assert!(red > 100, "{red}");
    // Ailleurs, rien de rouge.
    let (r, g, b) = pixel(&diff.image, 300, 200);
    assert!(r == g && g == b, "{:?}", (r, g, b));
}

#[test]
fn different_sizes_do_not_panic_and_count_the_extra_area() {
    let a = Document::from_bytes(sample_pdf(1, "a"), None).unwrap();
    let b = Document::from_bytes(blank_pdf(200.0, 200.0), None).unwrap();
    let diff = compare_pages(&a, 0, &b, 0, 1.0).unwrap();
    assert_eq!((diff.image.width_px, diff.image.height_px), (595, 842));
    assert_eq!(diff.total_pixels, 595 * 842);
    // Tout ce qui est hors de la petite page compte comme différent.
    assert!(diff.differing_pixels >= 595 * 842 - 200 * 200);
    let (r, g, b) = pixel(&diff.image, 400, 600);
    assert!(r > 200 && g < 80 && b < 80, "{:?}", (r, g, b));
}

#[test]
fn compare_rejects_bad_pages_and_scale() {
    let a = Document::from_bytes(sample_pdf(1, "a"), None).unwrap();
    assert!(matches!(
        compare_pages(&a, 3, &a, 0, 1.0),
        Err(CoreError::PageOutOfRange { .. })
    ));
    assert!(matches!(
        compare_pages(&a, 0, &a, 0, 0.0),
        Err(CoreError::InvalidOperation(_))
    ));
    assert!(matches!(
        compare_pages(&a, 0, &a, 0, 40.0),
        Err(CoreError::InvalidOperation(_))
    ));
}
