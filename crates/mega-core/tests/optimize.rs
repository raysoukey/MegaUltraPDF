// SPDX-License-Identifier: GPL-3.0-or-later
//! Optimisation : élagage, compression des flux, recompression et réduction des images.

use mega_core::edit::EditCommand;
use mega_core::optimize::OptimizeOptions;
use mega_core::testing::{heavy_images_pdf, sample_pdf};
use mega_core::{CoreError, Document};

fn heavy() -> Document {
    Document::from_bytes(heavy_images_pdf(), None).unwrap()
}

fn defaults() -> OptimizeOptions {
    OptimizeOptions {
        image_quality: Some(75),
        max_image_px: None,
        compress_streams: true,
    }
}

fn pixel(img: &mega_core::RenderedPage, x: u32, y: u32) -> (u8, u8, u8) {
    let i = ((y * img.width_px + x) * 4) as usize;
    (img.rgba[i], img.rgba[i + 1], img.rgba[i + 2])
}

/// Dictionnaires des images XObject du fichier : (largeur, filtre).
fn images_of(bytes: &[u8]) -> Vec<(i64, String)> {
    let lo = lopdf::Document::load_mem(bytes).unwrap();
    let mut out = Vec::new();
    for obj in lo.objects.values() {
        if let Ok(stream) = obj.as_stream()
            && stream.dict.get(b"Subtype").and_then(|s| s.as_name()).ok() == Some(b"Image")
        {
            let w = stream
                .dict
                .get(b"Width")
                .and_then(|w| w.as_i64())
                .unwrap_or(0);
            let f = stream
                .dict
                .get(b"Filter")
                .ok()
                .and_then(|f| f.as_name().ok())
                .map(|n| String::from_utf8_lossy(n).into_owned())
                .unwrap_or_default();
            out.push((w, f));
        }
    }
    out.sort();
    out
}

#[test]
fn heavy_fixture_renders_its_images() {
    let doc = heavy();
    let img = doc.render_page(0, 1.0).unwrap();
    // Centre de l'image rouge : (250, 550) en PDF → pixel (250, 842 − 550).
    let (r, g, b) = pixel(&img, 250, 842 - 550);
    assert!(r > 180 && g < 60 && b < 60, "{:?}", (r, g, b));
    // Trois images plus le masque alpha (lui aussi un XObject /Image).
    assert_eq!(images_of(&doc.to_bytes().unwrap()).len(), 4);
}

#[test]
fn optimize_recompresses_flate_rgb_image_and_keeps_rendering() {
    let mut doc = heavy();
    let before_bytes = doc.to_bytes().unwrap().len() as u64;
    let report = doc.optimize(&defaults()).unwrap();
    assert_eq!(report.bytes_before, before_bytes);
    assert!(
        report.bytes_after < report.bytes_before * 60 / 100,
        "{report:?}"
    );
    assert!(report.images_recompressed >= 1, "{report:?}");
    let images = images_of(&doc.to_bytes().unwrap());
    assert!(
        images.contains(&(800, "DCTDecode".to_string())),
        "l'image rouge est passée en JPEG : {images:?}"
    );
    let img = doc.render_page(0, 1.0).unwrap();
    let (r, g, b) = pixel(&img, 250, 842 - 550);
    assert!(
        (i16::from(r) - 200).abs() <= 12
            && (i16::from(g) - 30).abs() <= 12
            && (i16::from(b) - 30).abs() <= 12,
        "{:?}",
        (r, g, b)
    );
}

#[test]
fn optimize_downsamples_large_images() {
    let mut doc = heavy();
    let report = doc
        .optimize(&OptimizeOptions {
            image_quality: Some(80),
            max_image_px: Some(256),
            compress_streams: false,
        })
        .unwrap();
    let images = images_of(&doc.to_bytes().unwrap());
    assert!(
        images.iter().all(|(w, _)| *w <= 256),
        "toutes les images tiennent dans 256 px : {images:?} ({report:?})"
    );
}

#[test]
fn optimize_skips_masked_images() {
    let mut doc = heavy();
    doc.optimize(&defaults()).unwrap();
    let images = images_of(&doc.to_bytes().unwrap());
    // L'image masquée (8 px) garde son filtre d'origine (aucun / Flate), jamais DCT.
    let masked: Vec<_> = images.iter().filter(|(w, _)| *w == 8).collect();
    assert_eq!(masked.len(), 2, "image masquée + son masque : {images:?}");
    assert!(masked.iter().all(|(_, f)| f != "DCTDecode"), "{images:?}");
}

#[test]
fn optimize_never_grows_and_is_idempotent() {
    let mut doc = Document::from_bytes(sample_pdf(2, "léger"), None).unwrap();
    let first = doc.optimize(&defaults()).unwrap();
    assert!(first.bytes_after <= first.bytes_before, "{first:?}");
    assert_eq!(first.images_recompressed, 0);
    // Une seconde passe ne trouve plus d'image à recomprimer et ne grossit jamais
    // (PDFium ré-sérialise le document entre deux passes : quelques octets peuvent bouger).
    let second = doc.optimize(&defaults()).unwrap();
    assert!(second.bytes_after <= second.bytes_before, "{second:?}");
    assert_eq!(second.images_recompressed, 0, "{second:?}");
    assert_eq!(doc.page_count(), 2);
    assert_eq!(
        doc.text_boxes(1)
            .unwrap()
            .iter()
            .map(|b| b.ch)
            .collect::<String>()
            .trim(),
        "Page 2"
    );
}

#[test]
fn optimize_is_an_undoable_command_and_refuses_encrypted() {
    let mut doc = heavy();
    let before = doc.render_page(0, 0.5).unwrap();
    doc.apply(&EditCommand::Optimize {
        options: OptimizeOptions {
            image_quality: Some(40),
            max_image_px: Some(300),
            compress_streams: true,
        },
    })
    .unwrap();
    assert_eq!(doc.undo_label().as_deref(), Some("Optimiser"));
    assert!(doc.is_modified());
    doc.undo().unwrap();
    assert_eq!(doc.render_page(0, 0.5).unwrap().rgba, before.rgba);
    assert!(matches!(
        doc.optimize(&OptimizeOptions {
            image_quality: Some(5),
            ..defaults()
        }),
        Err(CoreError::InvalidOperation(_))
    ));
    let bytes = std::fs::read(format!(
        "{}/../../fixtures/owner-only.pdf",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let mut protected = Document::from_bytes(bytes, None).unwrap();
    match protected.optimize(&defaults()) {
        Err(CoreError::InvalidOperation(m)) => assert!(m.contains("chiffré"), "{m}"),
        other => panic!("{other:?}"),
    }
}
