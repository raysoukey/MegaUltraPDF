// SPDX-License-Identifier: GPL-3.0-or-later
//! Chaque variante d'`EditCommand`, écrite en JSON comme l'interface l'envoie, produit un
//! fichier que qpdf accepte ; et aucune variante n'est oubliée.

use std::path::Path;

use mega_core::Document;
use serde_json::{Value, json};

use crate::{
    Case, annotation, annotation_with, assert_conforms, assert_text_reads_back, check, fixture,
    missing_variants, qpdf, rect, style,
};

/// Éditions de pages et de document sur `simple.pdf`.
fn page_cases() -> Vec<Case> {
    let simple = "simple.pdf";
    vec![
        Case::new(
            "pivoter",
            simple,
            vec![json!({"kind": "rotate_pages", "pages": [0, 2], "quarter_turns": 1})],
        ),
        Case::new(
            "supprimer",
            simple,
            vec![json!({"kind": "delete_pages", "pages": [1]})],
        ),
        Case::new(
            "déplacer",
            simple,
            vec![json!({"kind": "move_pages", "pages": [2], "to": 0})],
        ),
        Case::new(
            "page blanche",
            simple,
            vec![json!({"kind": "insert_blank_page", "at": 1,
                        "size": {"width_pt": 595.0, "height_pt": 842.0}})],
        ),
        Case::new(
            "insérer un fichier",
            simple,
            vec![json!({"kind": "insert_pages_from_file",
                        "path": fixture("rotated.pdf").to_string_lossy(),
                        "pages": null, "at": 3})],
        ),
        Case::new(
            "filigrane",
            simple,
            vec![
                json!({"kind": "watermark", "spec": {"text": "CONFIDENTIEL — été", "size": 60.0,
                        "opacity": 0.3, "angle_deg": 45.0, "color": {"r": 200, "g": 0, "b": 0},
                        "pages": null}}),
            ],
        ),
        Case::new(
            "numéros de page",
            simple,
            vec![
                json!({"kind": "page_numbers", "spec": {"template": "Page {n} / {total}",
                        "corner": "bottom_center", "margin": 20.0, "size": 10.0, "start": 1,
                        "pages": null}}),
            ],
        ),
        Case::new(
            "recadrer",
            simple,
            vec![json!({"kind": "crop_pages", "spec": {"pages": [0],
                        "margins": [30.0, 30.0, 30.0, 30.0]}})],
        ),
        Case::new(
            "métadonnées",
            simple,
            vec![
                json!({"kind": "set_metadata", "patch": {"title": "Titre accentué ©",
                        "author": "Zoé", "keywords": "audit, qpdf"}}),
            ],
        ),
        Case::new(
            "optimiser",
            simple,
            vec![json!({"kind": "optimize", "options": {"image_quality": 75,
                        "max_image_px": null, "compress_streams": true}})],
        ),
        Case::new(
            "protéger",
            simple,
            vec![
                json!({"kind": "protect", "protection": {"user_password": "u",
                        "owner_password": "o", "permissions": {"print": true, "copy": false,
                        "modify": false, "annotate": true}}}),
            ],
        ),
    ]
}

/// Images de test écrites dans `dir` : RVB opaque, RVBA à transparence réelle, JPEG.
struct Images {
    png: String,
    png_alpha: String,
    jpg: String,
}

fn images(dir: &Path) -> Images {
    let png = dir.join("image.png");
    image::RgbImage::from_pixel(40, 20, image::Rgb([10, 120, 200]))
        .save(&png)
        .unwrap();
    // E9 (Lot 14) : une image transparente (vrai `/SMask`) et un JPEG gardé en `/DCTDecode`.
    let png_alpha = dir.join("image-alpha.png");
    let mut rgba = image::RgbaImage::new(40, 20);
    for (x, _y, p) in rgba.enumerate_pixels_mut() {
        *p = image::Rgba([20, 120, 200, if x < 20 { 255 } else { 0 }]);
    }
    rgba.save(&png_alpha).unwrap();
    let jpg = dir.join("image.jpg");
    image::RgbImage::from_pixel(40, 20, image::Rgb([200, 30, 30]))
        .save(&jpg)
        .unwrap();
    let text = |p: std::path::PathBuf| p.to_string_lossy().into_owned();
    Images {
        png: text(png),
        png_alpha: text(png_alpha),
        jpg: text(jpg),
    }
}

/// Annotations, contenu, formulaires et pièces jointes (Lot 13).
fn content_cases(dir: &Path) -> Vec<Case> {
    let img = images(dir);
    let attached = dir.join("note.txt");
    std::fs::write(&attached, "pièce jointe").unwrap();
    vec![
        Case::new(
            "rectangle",
            "simple.pdf",
            vec![annotation(
                "rect",
                json!({"kind": "rect", "rect": rect(100.0, 500.0, 200.0, 100.0)}),
            )],
        ),
        Case::new(
            "note",
            "simple.pdf",
            vec![annotation(
                "note",
                json!({"kind": "rect", "rect": rect(80.0, 700.0, 20.0, 20.0)}),
            )],
        ),
        Case::new(
            "surlignage",
            "out-of-order.pdf",
            vec![annotation(
                "highlight",
                json!({"kind": "quads", "rects": [rect(70.0, 640.0, 200.0, 14.0)]}),
            )],
        ),
        Case::new(
            "encre",
            "simple.pdf",
            vec![annotation(
                "ink",
                json!({"kind": "strokes", "strokes": [[[100.0, 100.0], [150.0, 160.0], [200.0, 120.0]]]}),
            )],
        ),
        Case::new(
            "ligne",
            "simple.pdf",
            vec![annotation(
                "line",
                json!({"kind": "line", "from": [100.0, 300.0], "to": [300.0, 350.0]}),
            )],
        ),
        Case::new(
            "texte libre",
            "simple.pdf",
            vec![annotation(
                "text",
                json!({"kind": "rect", "rect": rect(100.0, 400.0, 200.0, 40.0)}),
            )],
        ),
        Case::new(
            "modifier puis supprimer une annotation",
            "annotated.pdf",
            vec![
                json!({"kind": "update_annotation", "page": 0, "index": 0, "patch": {"contents": "modifiée"}}),
                json!({"kind": "move_annotation", "page": 0, "index": 0, "rect": rect(50.0, 50.0, 100.0, 50.0)}),
                json!({"kind": "delete_annotation", "page": 0, "index": 1}),
            ],
        ),
        Case::new(
            "texte inséré (Unicode)",
            "simple.pdf",
            vec![
                json!({"kind": "insert_text", "page": 0, "at": [100.0, 600.0], "text": "Ελληνικά — Ünïcödé",
                        "style": {"family": "unicode", "bold": false, "italic": false, "size": 14.0,
                                  "color": {"r": 0, "g": 0, "b": 0}}}),
            ],
        )
        .expecting("Ελληνικά — Ünïcödé"),
        Case::new(
            // E10 (Lot 14) : la police Unicode incorporée par PDFium sort en sous-ensemble.
            "tampon Unicode",
            "simple.pdf",
            vec![json!({"kind": "add_annotation", "page": 0, "annotation": {
                "kind": "stamp", "geometry": {"kind": "rect", "rect": rect(100.0, 400.0, 200.0, 40.0)},
                "style": style(), "contents": "Ωμέγα Жук", "author": "Moi"}})],
        )
        .expecting("Ωμέγα Жук"),
        Case::new(
            "image insérée",
            "simple.pdf",
            vec![
                json!({"kind": "insert_image", "page": 0, "rect": rect(100.0, 100.0, 120.0, 60.0),
                        "path": img.png}),
            ],
        ),
        Case::new(
            "image insérée transparente (PNG RVBA)",
            "simple.pdf",
            vec![
                json!({"kind": "insert_image", "page": 0, "rect": rect(100.0, 100.0, 120.0, 60.0),
                        "path": img.png_alpha}),
            ],
        ),
        Case::new(
            "image insérée JPEG sur page tournée",
            "rotated.pdf",
            vec![
                json!({"kind": "insert_image", "page": 0, "rect": rect(50.0, 50.0, 120.0, 60.0),
                        "path": img.jpg}),
            ],
        ),
        Case::new(
            "objet déplacé, redimensionné, supprimé",
            "simple.pdf",
            vec![
                json!({"kind": "move_object", "page": 0, "index": 0, "dx": 10.0, "dy": -5.0}),
                json!({"kind": "resize_object", "page": 0, "index": 0, "rect": rect(60.0, 700.0, 300.0, 40.0)}),
                json!({"kind": "delete_object", "page": 1, "index": 0}),
            ],
        ),
        Case::new(
            "retouche de texte",
            "simple.pdf",
            vec![json!({"kind": "set_object_text", "page": 0, "index": 0, "text": "Retouché"})],
        ),
        Case::new(
            "caviardage",
            "out-of-order.pdf",
            vec![json!({"kind": "redact", "page": 0, "rect": rect(60.0, 600.0, 300.0, 60.0)})],
        ),
        Case::new(
            "caviardage d'image",
            "image-mosaic.pdf",
            vec![json!({"kind": "redact", "page": 0, "rect": rect(150.0, 400.0, 100.0, 100.0)})],
        ),
        Case::new(
            "signature visuelle",
            "simple.pdf",
            vec![
                json!({"kind": "insert_signature", "page": 0, "rect": rect(350.0, 100.0, 150.0, 50.0),
                        "source": {"kind": "strokes", "strokes": [[[360.0, 110.0], [400.0, 130.0], [450.0, 115.0]]]}}),
            ],
        ),
        Case::new(
            "pièces jointes",
            "attachments.pdf",
            vec![
                json!({"kind": "add_attachment", "path": attached.to_string_lossy()}),
                json!({"kind": "delete_attachment", "index": 0}),
            ],
        ),
        Case::new(
            "formulaire rempli",
            "form.pdf",
            vec![
                json!({"kind": "set_field_value", "page": 0, "index": 0, "value": {"kind": "text", "text": "Dupont é"}}),
            ],
        ),
        Case::new(
            "formulaire aplati",
            "form.pdf",
            vec![json!({"kind": "flatten_form"})],
        ),
    ]
}

#[test]
fn page_editions_produce_conforming_files() {
    let Some(qpdf) = qpdf() else { return };
    for case in page_cases() {
        check(&qpdf, &case);
    }
}

#[test]
fn annotations_and_content_produce_conforming_files() {
    let Some(qpdf) = qpdf() else { return };
    let dir = tempfile::tempdir().unwrap();
    for case in content_cases(dir.path()) {
        check(&qpdf, &case);
    }
}

/// La couche de texte invisible de l'OCR (sans Tesseract : les mots sont fournis), en
/// Helvetica pour un mot WinAnsi et en police Unicode (`Type0Writer`) pour les autres.
#[test]
fn ocr_text_layers_produce_conforming_files() {
    let Some(qpdf) = qpdf() else { return };
    let dir = tempfile::tempdir().unwrap();
    for (what, text) in [
        ("couche OCR (WinAnsi)", "Reconnu"),
        ("couche OCR (Unicode)", "Ωμέγα Жук"),
    ] {
        let mut doc = Document::open(fixture("scanned.pdf"), None).unwrap();
        doc.add_invisible_words(
            0,
            &[mega_core::overlay::InvisibleWord {
                text: text.to_string(),
                rect: mega_core::Rect {
                    x: 100.0,
                    y: 600.0,
                    w: 80.0,
                    h: 14.0,
                },
            }],
        )
        .unwrap();
        let out = dir.path().join("ocr.pdf");
        doc.save_as(&out).unwrap();
        assert_conforms(&qpdf, &out, None, what);
        assert_text_reads_back(&out, None, text, what);
    }
}

// --- Lot 14, audit E1 : les commandes que le §5 du rapport d'audit listait hors qpdf ------

/// Case, bouton radio et liste déroulante de `form.pdf` (index 2, 4 et 5 : voir
/// `tests/forms.rs`).
fn form_widget_cases() -> Vec<Case> {
    vec![
        Case::new(
            "case cochée",
            "form.pdf",
            vec![
                json!({"kind": "set_field_value", "page": 0, "index": 2, "value": {"kind": "checked", "checked": true}}),
            ],
        ),
        Case::new(
            "bouton radio B",
            "form.pdf",
            vec![
                json!({"kind": "set_field_value", "page": 0, "index": 4, "value": {"kind": "checked", "checked": true}}),
            ],
        ),
        Case::new(
            "liste déroulante",
            "form.pdf",
            vec![
                json!({"kind": "set_field_value", "page": 0, "index": 5, "value": {"kind": "text", "text": "Lyon"}}),
            ],
        ),
    ]
}

/// Les genres d'annotation que le Lot 13 ne contrôlait pas : formes, flèche, marquages de
/// texte, tampon au texte ASCII (le tampon Unicode est dans [`content_cases`]).
fn annotation_kind_cases() -> Vec<Case> {
    let words = || json!({"kind": "quads", "rects": [rect(70.0, 640.0, 200.0, 14.0)]});
    vec![
        Case::new(
            "ellipse",
            "simple.pdf",
            vec![annotation(
                "ellipse",
                json!({"kind": "rect", "rect": rect(100.0, 500.0, 200.0, 100.0)}),
            )],
        ),
        Case::new(
            "flèche",
            "simple.pdf",
            vec![annotation(
                "arrow",
                json!({"kind": "line", "from": [100.0, 300.0], "to": [300.0, 350.0]}),
            )],
        ),
        Case::new(
            "souligné",
            "out-of-order.pdf",
            vec![annotation("underline", words())],
        ),
        Case::new(
            "barré",
            "out-of-order.pdf",
            vec![annotation("strike_out", words())],
        ),
        Case::new(
            "ondulé",
            "out-of-order.pdf",
            vec![annotation("squiggly", words())],
        ),
        Case::new(
            "tampon ASCII",
            "simple.pdf",
            vec![annotation_with(
                "stamp",
                json!({"kind": "rect", "rect": rect(100.0, 400.0, 200.0, 40.0)}),
                "APPROVED",
            )],
        ),
    ]
}

/// Ajouts de texte et d'images : filigrane et numéros hors WinAnsi (police Unicode écrite par
/// `Type0Writer`), texte Helvetica, retouche qui passe en DejaVu, images JPEG et PNG sur une
/// page droite, signature visuelle tirée d'une image.
fn text_and_image_cases(dir: &Path) -> Vec<Case> {
    let img = images(dir);
    vec![
        Case::new(
            "filigrane Unicode",
            "simple.pdf",
            vec![
                json!({"kind": "watermark", "spec": {"text": "ΠΡΟΣΧΈΔΙΟ — Черновик", "size": 50.0,
                        "opacity": 0.3, "angle_deg": 45.0, "color": {"r": 200, "g": 0, "b": 0},
                        "pages": null}}),
            ],
        )
        .expecting("Черновик"),
        Case::new(
            "numéros de page Unicode",
            "simple.pdf",
            vec![
                json!({"kind": "page_numbers", "spec": {"template": "Страница {n} / {total}",
                        "corner": "bottom_right", "margin": 20.0, "size": 10.0, "start": 1,
                        "pages": null}}),
            ],
        )
        .expecting("Страница 1 / 3"),
        Case::new(
            "texte inséré (Helvetica)",
            "simple.pdf",
            vec![
                json!({"kind": "insert_text", "page": 0, "at": [100.0, 600.0], "text": "Texte ajouté — été",
                        "style": {"family": "helvetica", "bold": true, "italic": false, "size": 14.0,
                                  "color": {"r": 0, "g": 0, "b": 120}}}),
            ],
        )
        .expecting("Texte ajouté — été"),
        Case::new(
            "retouche de texte vers DejaVu",
            "simple.pdf",
            vec![json!({"kind": "set_object_text", "page": 0, "index": 0, "text": "Страница 1"})],
        )
        .expecting("Страница 1"),
        Case::new(
            "image insérée JPEG",
            "simple.pdf",
            vec![
                json!({"kind": "insert_image", "page": 0, "rect": rect(100.0, 100.0, 120.0, 60.0),
                        "path": img.jpg}),
            ],
        ),
        Case::new(
            "image insérée PNG",
            "simple.pdf",
            vec![
                json!({"kind": "insert_image", "page": 0, "rect": rect(300.0, 100.0, 120.0, 60.0),
                        "path": img.png}),
            ],
        ),
        Case::new(
            "signature visuelle (image)",
            "simple.pdf",
            vec![
                json!({"kind": "insert_signature", "page": 0, "rect": rect(350.0, 100.0, 150.0, 50.0),
                        "source": {"kind": "image", "path": img.png_alpha}}),
            ],
        ),
    ]
}

/// Rotation de la page 1, filigrane translucide et note : trois écritures (PDFium et `lopdf`)
/// sur un même fichier.
fn rotate_watermark_note() -> Vec<Value> {
    vec![
        json!({"kind": "rotate_pages", "pages": [0], "quarter_turns": 1}),
        json!({"kind": "watermark", "spec": {"text": "COPIE", "size": 60.0, "opacity": 0.3,
                "angle_deg": 45.0, "color": {"r": 0, "g": 0, "b": 200}, "pages": null}}),
        annotation(
            "note",
            json!({"kind": "rect", "rect": rect(80.0, 700.0, 20.0, 20.0)}),
        ),
    ]
}

/// Fichiers chiffrés : `encrypted.pdf` (mot de passe d'ouverture `secret`, que qpdf reçoit
/// aussi pour relire la sortie, toujours chiffrée) et `owner-only.pdf` (mot de passe
/// propriétaire `secret` seulement : s'ouvre sans mot de passe). Éditer garde la protection ;
/// `unprotect` la retire (avec le mot de passe propriétaire pour `owner-only.pdf`).
fn encrypted_cases() -> Vec<Case> {
    let with = |what, source: &str, password, commands| Case {
        what,
        source: fixture(source),
        password,
        commands,
        expect_text: None,
    };
    vec![
        with(
            "chiffré (ouverture) : rotation, filigrane, note",
            "encrypted.pdf",
            Some("secret"),
            rotate_watermark_note(),
        ),
        with(
            "chiffré (propriétaire) : rotation, filigrane, note",
            "owner-only.pdf",
            None,
            rotate_watermark_note(),
        ),
        // Expérience E9 du banc de l'audit : le caviardage réécrit le contenu de la page, puis
        // le fichier est rechiffré.
        with(
            "chiffré (ouverture) : caviardage",
            "encrypted.pdf",
            Some("secret"),
            vec![json!({"kind": "redact", "page": 0, "rect": rect(60.0, 740.0, 300.0, 60.0)})],
        ),
        with(
            "protection retirée (ouverture)",
            "encrypted.pdf",
            Some("secret"),
            vec![json!({"kind": "unprotect", "password": null})],
        ),
        with(
            "protection retirée (propriétaire)",
            "owner-only.pdf",
            Some("secret"),
            vec![json!({"kind": "unprotect", "password": null})],
        ),
    ]
}

#[test]
fn form_widgets_produce_conforming_files() {
    let Some(qpdf) = qpdf() else { return };
    for case in form_widget_cases() {
        check(&qpdf, &case);
    }
}

#[test]
fn every_annotation_kind_produces_a_conforming_file() {
    let Some(qpdf) = qpdf() else { return };
    for case in annotation_kind_cases() {
        check(&qpdf, &case);
    }
}

#[test]
fn unicode_text_and_images_produce_conforming_files() {
    let Some(qpdf) = qpdf() else { return };
    let dir = tempfile::tempdir().unwrap();
    for case in text_and_image_cases(dir.path()) {
        check(&qpdf, &case);
    }
}

/// Éditer un fichier chiffré garde la protection (`/Encrypt`), la retirer l'enlève ; les deux
/// sorties sont conformes.
#[test]
fn editing_encrypted_files_produces_conforming_files() {
    let Some(qpdf) = qpdf() else { return };
    for case in encrypted_cases() {
        let out = check(&qpdf, &case);
        let protected = case.commands.iter().all(|c| c["kind"] != "unprotect");
        assert_eq!(
            String::from_utf8_lossy(&out).contains("/Encrypt"),
            protected,
            "{} : protection {}",
            case.what,
            if protected { "perdue" } else { "restée" }
        );
    }
}

/// L'optimisation **recomprime vraiment** (`heavy_images_pdf` : une grande image Flate
/// bruitée), et le fichier réécrit reste conforme.
#[test]
fn optimizing_heavy_images_produces_a_smaller_conforming_file() {
    let Some(qpdf) = qpdf() else { return };
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("lourd.pdf");
    let heavy = mega_core::testing::heavy_images_pdf();
    std::fs::write(&source, &heavy).unwrap();
    let case = Case {
        what: "optimiser des images lourdes",
        source,
        password: None,
        commands: vec![json!({"kind": "optimize", "options": {"image_quality": 60,
                              "max_image_px": 400, "compress_streams": true}})],
        expect_text: None,
    };
    let out = check(&qpdf, &case);
    assert!(
        out.len() < heavy.len() / 2,
        "recompression sans effet : {} → {} octets",
        heavy.len(),
        out.len()
    );
}

/// Aucune variante d'`EditCommand` n'échappe à qpdf : les listes de cas ci-dessus les
/// exercent toutes (hors `ocr_page`, voir `expected_variants`). Une variante ajoutée sans cas
/// fait échouer ce test, qui la nomme.
#[test]
fn every_edit_command_variant_is_checked_by_qpdf() {
    let dir = tempfile::tempdir().unwrap();
    let cases: Vec<Case> = page_cases()
        .into_iter()
        .chain(content_cases(dir.path()))
        .chain(form_widget_cases())
        .chain(annotation_kind_cases())
        .chain(text_and_image_cases(dir.path()))
        .chain(encrypted_cases())
        .collect();
    let missing = missing_variants(cases.iter().flat_map(|c| c.commands.iter()));
    assert!(
        missing.is_empty(),
        "variantes sans contrôle qpdf : {missing:?}"
    );
}
