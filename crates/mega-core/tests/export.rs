// SPDX-License-Identifier: GPL-3.0-or-later
//! Export d'un document en images (PNG / JPEG, résolution choisie) et en texte.

use mega_core::export::{ImageExport, ImageFormat};
use mega_core::testing::{blank_pdf, sample_pdf};
use mega_core::{CoreError, Document};

fn three() -> Document {
    Document::from_bytes(sample_pdf(3, "export"), None).unwrap()
}

fn spec(format: ImageFormat, dpi: u16) -> ImageExport {
    ImageExport {
        pages: None,
        format,
        dpi,
        with_annotations: true,
    }
}

#[test]
fn export_images_writes_one_png_per_page() {
    let doc = three();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("sortie");
    let files = doc
        .export_images(&spec(ImageFormat::Png, 72), &out, "page", false)
        .unwrap();
    assert_eq!(files.len(), 3);
    assert_eq!(files[0].file_name().unwrap(), "page-1.png");
    assert_eq!(files[2].file_name().unwrap(), "page-3.png");
    let img = image::open(&files[0]).unwrap();
    assert_eq!((img.width(), img.height()), (595, 842));
}

#[test]
fn export_images_jpeg_quality_and_dpi() {
    let doc = three();
    let dir = tempfile::tempdir().unwrap();
    let low = doc
        .export_images(
            &ImageExport {
                pages: Some(vec![1]),
                ..spec(ImageFormat::Jpeg { quality: 30 }, 150)
            },
            &dir.path().join("bas"),
            "p",
            false,
        )
        .unwrap();
    let high = doc
        .export_images(
            &ImageExport {
                pages: Some(vec![1]),
                ..spec(ImageFormat::Jpeg { quality: 90 }, 150)
            },
            &dir.path().join("haut"),
            "p",
            false,
        )
        .unwrap();
    assert_eq!(low.len(), 1);
    assert_eq!(
        low[0].file_name().unwrap(),
        "p-2.jpg",
        "numéro de page, pas index"
    );
    let img = image::open(&low[0]).unwrap();
    assert_eq!((img.width(), img.height()), (1240, 1754));
    let size = |p: &std::path::Path| std::fs::metadata(p).unwrap().len();
    assert!(size(&low[0]) < size(&high[0]));
}

#[test]
fn export_refuses_overwrite_without_force() {
    let doc = three();
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("page-2.png");
    std::fs::write(&existing, b"ne pas toucher").unwrap();
    let err = doc
        .export_images(&spec(ImageFormat::Png, 72), dir.path(), "page", false)
        .unwrap_err();
    assert!(matches!(err, CoreError::InvalidOperation(_)), "{err:?}");
    assert_eq!(std::fs::read(&existing).unwrap(), b"ne pas toucher");
    assert!(
        !dir.path().join("page-1.png").exists(),
        "rien n'est écrit avant la vérification"
    );
    let files = doc
        .export_images(&spec(ImageFormat::Png, 72), dir.path(), "page", true)
        .unwrap();
    assert_eq!(files.len(), 3);
    assert_ne!(std::fs::read(&existing).unwrap(), b"ne pas toucher");
}

#[test]
fn export_rejects_bad_dpi_and_pixel_budget() {
    let dir = tempfile::tempdir().unwrap();
    let doc = three();
    for dpi in [20, 700] {
        assert!(
            matches!(
                doc.export_images(&spec(ImageFormat::Png, dpi), dir.path(), "x", true),
                Err(CoreError::InvalidOperation(_))
            ),
            "dpi {dpi}"
        );
    }
    let huge = Document::from_bytes(blank_pdf(2000.0, 2000.0), None).unwrap();
    // 2000 pt × 600 / 72 = 16 667 px de côté : 277 Mpx, au-delà du budget.
    assert!(matches!(
        huge.export_images(&spec(ImageFormat::Png, 600), dir.path(), "x", true),
        Err(CoreError::InvalidOperation(_))
    ));
    assert!(matches!(
        doc.export_images(
            &spec(ImageFormat::Jpeg { quality: 5 }, 72),
            dir.path(),
            "x",
            true
        ),
        Err(CoreError::InvalidOperation(_))
    ));
    assert!(matches!(
        doc.export_images(
            &ImageExport {
                pages: Some(vec![7]),
                ..spec(ImageFormat::Png, 72)
            },
            dir.path(),
            "x",
            true
        ),
        Err(CoreError::PageOutOfRange { .. })
    ));
}

#[test]
fn page_text_and_export_text() {
    let doc = Document::from_bytes(sample_pdf(2, "texte"), None).unwrap();
    assert_eq!(doc.page_text(0).unwrap().trim(), "Page 1");
    let all = doc.export_text(None, true).unwrap();
    assert!(all.contains("===== Page 1 =====") && all.contains("===== Page 2 ====="));
    assert!(all.contains("Page 1") && all.contains("Page 2"));
    let plain = doc.export_text(None, false).unwrap();
    assert!(!plain.contains("====="));
    let second = doc.export_text(Some(&[1]), false).unwrap();
    assert!(second.contains("Page 2") && !second.contains("Page 1"));
    assert!(matches!(
        doc.page_text(5),
        Err(CoreError::PageOutOfRange { .. })
    ));
}

/// M3 (relecture) : sous Windows, `dir.join("C:x-1.png")` sort du dossier choisi et
/// `a:b` crée un flux NTFS ; le préfixe refuse tout caractère interdit dans un nom.
#[test]
fn export_refuses_prefix_with_forbidden_characters() {
    let doc = three();
    let dir = tempfile::tempdir().unwrap();
    for bad in [
        "C:x", "a:b", "x*y", "a?b", "a\"b", "a<b", "a>b", "a|b", "a\u{7}b",
    ] {
        assert!(
            matches!(
                doc.export_images(&spec(ImageFormat::Png, 72), dir.path(), bad, true),
                Err(CoreError::InvalidOperation(_))
            ),
            "{bad:?}"
        );
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}
