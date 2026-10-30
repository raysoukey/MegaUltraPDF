// SPDX-License-Identifier: GPL-3.0-or-later
//! Nos fichiers s'ouvrent-ils **ailleurs** ? (Lot 13, audit C1)
//!
//! Chaque édition est appliquée à un vrai fichier, le résultat est enregistré, puis contrôlé
//! par `qpdf --check`, un validateur indépendant de PDFium et de lopdf : syntaxe, table de
//! références, flux. Un avertissement compte comme un échec : un autre lecteur pourrait ne
//! pas réparer ce que qpdf signale.
//!
//! qpdf est un outil externe : sans lui, ces tests se signalent et passent, sauf si la
//! variable `MEGA_REQUIRE_QPDF=1` est posée (la CI la pose). L'exécutable est pris dans
//! `MEGA_QPDF`, sinon dans le `PATH`.
//!
//! Les commandes sont écrites en JSON, exactement comme l'interface les envoie
//! (`app/src/lib/api.ts`) : le test couvre aussi leur forme sérialisée.

use std::path::{Path, PathBuf};
use std::process::Command;

use mega_core::Document;
use mega_core::edit::EditCommand;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(format!(
        "{}/../../fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
}

/// L'exécutable qpdf, s'il répond ; sans lui, panique seulement si la CI l'exige.
fn qpdf() -> Option<PathBuf> {
    let exe = std::env::var_os("MEGA_QPDF").map_or_else(|| PathBuf::from("qpdf"), PathBuf::from);
    let works = Command::new(&exe)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if works {
        return Some(exe);
    }
    assert!(
        std::env::var("MEGA_REQUIRE_QPDF").as_deref() != Ok("1"),
        "qpdf introuvable alors que MEGA_REQUIRE_QPDF=1"
    );
    eprintln!("qpdf introuvable : contrôle de conformité sauté");
    None
}

/// `qpdf --check` sur `path` (avec `password` s'il est chiffré) : sans erreur **ni**
/// avertissement.
fn assert_conforms(qpdf: &Path, path: &Path, password: Option<&str>, what: &str) {
    let mut cmd = Command::new(qpdf);
    if let Some(p) = password {
        cmd.arg(format!("--password={p}"));
    }
    let out = cmd.arg("--check").arg(path).output().unwrap();
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.status.code() == Some(0) && !report.contains("WARNING"),
        "{what} : qpdf --check a échoué ({:?})\n{report}",
        out.status.code()
    );
}

/// Applique `commands` (JSON) à `source`, enregistre, contrôle avec qpdf.
fn check(qpdf: &Path, source: &str, what: &str, commands: &[serde_json::Value]) {
    let dir = tempfile::tempdir().unwrap();
    let mut doc = Document::open(fixture(source), None).unwrap();
    for json in commands {
        let command: EditCommand = serde_json::from_value(json.clone())
            .unwrap_or_else(|e| panic!("{what} : commande {json} illisible : {e}"));
        doc.apply(&command)
            .unwrap_or_else(|e| panic!("{what} : {json} refusée : {e}"));
    }
    let out = dir.path().join("sortie.pdf");
    doc.save_as(&out).unwrap();
    let password = commands
        .iter()
        .find_map(|c| c["protection"]["user_password"].as_str());
    assert_conforms(qpdf, &out, password, what);
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> serde_json::Value {
    serde_json::json!({ "x": x, "y": y, "w": w, "h": h })
}

fn style() -> serde_json::Value {
    serde_json::json!({
        "color": { "r": 200, "g": 0, "b": 0 },
        "fill": null,
        "opacity": 1.0,
        "stroke_width": 2.0,
        "font_size": 12.0
    })
}

fn annotation(kind: &str, geometry: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "kind": "add_annotation",
        "page": 0,
        "annotation": {
            "kind": kind,
            "geometry": geometry,
            "style": style(),
            "contents": "Contrôle de conformité",
            "author": "Moi"
        }
    })
}

#[test]
fn page_editions_produce_conforming_files() {
    let Some(qpdf) = qpdf() else { return };
    use serde_json::json;
    let simple = "simple.pdf";
    let cases: Vec<(&str, Vec<serde_json::Value>)> = vec![
        (
            "pivoter",
            vec![json!({"kind": "rotate_pages", "pages": [0, 2], "quarter_turns": 1})],
        ),
        (
            "supprimer",
            vec![json!({"kind": "delete_pages", "pages": [1]})],
        ),
        (
            "déplacer",
            vec![json!({"kind": "move_pages", "pages": [2], "to": 0})],
        ),
        (
            "page blanche",
            vec![json!({"kind": "insert_blank_page", "at": 1,
                        "size": {"width_pt": 595.0, "height_pt": 842.0}})],
        ),
        (
            "insérer un fichier",
            vec![json!({"kind": "insert_pages_from_file",
                        "path": fixture("rotated.pdf").to_string_lossy(),
                        "pages": null, "at": 3})],
        ),
        (
            "filigrane",
            vec![
                json!({"kind": "watermark", "spec": {"text": "CONFIDENTIEL — été", "size": 60.0,
                        "opacity": 0.3, "angle_deg": 45.0, "color": {"r": 200, "g": 0, "b": 0},
                        "pages": null}}),
            ],
        ),
        (
            "numéros de page",
            vec![
                json!({"kind": "page_numbers", "spec": {"template": "Page {n} / {total}",
                        "corner": "bottom_center", "margin": 20.0, "size": 10.0, "start": 1,
                        "pages": null}}),
            ],
        ),
        (
            "recadrer",
            vec![json!({"kind": "crop_pages", "spec": {"pages": [0],
                        "margins": [30.0, 30.0, 30.0, 30.0]}})],
        ),
        (
            "métadonnées",
            vec![
                json!({"kind": "set_metadata", "patch": {"title": "Titre accentué ©",
                        "author": "Zoé", "keywords": "audit, qpdf"}}),
            ],
        ),
        (
            "optimiser",
            vec![json!({"kind": "optimize", "options": {"image_quality": 75,
                        "max_image_px": null, "compress_streams": true}})],
        ),
        (
            "protéger",
            vec![
                json!({"kind": "protect", "protection": {"user_password": "u",
                        "owner_password": "o", "permissions": {"print": true, "copy": false,
                        "modify": false, "annotate": true}}}),
            ],
        ),
    ];
    for (what, commands) in cases {
        check(&qpdf, simple, what, &commands);
    }
}

#[test]
fn annotations_and_content_produce_conforming_files() {
    let Some(qpdf) = qpdf() else { return };
    use serde_json::json;
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("image.png");
    image::RgbImage::from_pixel(40, 20, image::Rgb([10, 120, 200]))
        .save(&png)
        .unwrap();
    let attached = dir.path().join("note.txt");
    std::fs::write(&attached, "pièce jointe").unwrap();
    let cases: Vec<(&str, &str, Vec<serde_json::Value>)> = vec![
        (
            "rectangle",
            "simple.pdf",
            vec![annotation(
                "rect",
                json!({"kind": "rect", "rect": rect(100.0, 500.0, 200.0, 100.0)}),
            )],
        ),
        (
            "note",
            "simple.pdf",
            vec![annotation(
                "note",
                json!({"kind": "rect", "rect": rect(80.0, 700.0, 20.0, 20.0)}),
            )],
        ),
        (
            "surlignage",
            "out-of-order.pdf",
            vec![annotation(
                "highlight",
                json!({"kind": "quads", "rects": [rect(70.0, 640.0, 200.0, 14.0)]}),
            )],
        ),
        (
            "encre",
            "simple.pdf",
            vec![annotation(
                "ink",
                json!({"kind": "strokes", "strokes": [[[100.0, 100.0], [150.0, 160.0], [200.0, 120.0]]]}),
            )],
        ),
        (
            "ligne",
            "simple.pdf",
            vec![annotation(
                "line",
                json!({"kind": "line", "from": [100.0, 300.0], "to": [300.0, 350.0]}),
            )],
        ),
        (
            "texte libre",
            "simple.pdf",
            vec![annotation(
                "text",
                json!({"kind": "rect", "rect": rect(100.0, 400.0, 200.0, 40.0)}),
            )],
        ),
        (
            "modifier puis supprimer une annotation",
            "annotated.pdf",
            vec![
                json!({"kind": "update_annotation", "page": 0, "index": 0, "patch": {"contents": "modifiée"}}),
                json!({"kind": "move_annotation", "page": 0, "index": 0, "rect": rect(50.0, 50.0, 100.0, 50.0)}),
                json!({"kind": "delete_annotation", "page": 0, "index": 1}),
            ],
        ),
        (
            "texte inséré (Unicode)",
            "simple.pdf",
            vec![
                json!({"kind": "insert_text", "page": 0, "at": [100.0, 600.0], "text": "Ελληνικά — Ünïcödé",
                        "style": {"family": "unicode", "bold": false, "italic": false, "size": 14.0,
                                  "color": {"r": 0, "g": 0, "b": 0}}}),
            ],
        ),
        (
            "image insérée",
            "simple.pdf",
            vec![
                json!({"kind": "insert_image", "page": 0, "rect": rect(100.0, 100.0, 120.0, 60.0),
                        "path": png.to_string_lossy()}),
            ],
        ),
        (
            "objet déplacé, redimensionné, supprimé",
            "simple.pdf",
            vec![
                json!({"kind": "move_object", "page": 0, "index": 0, "dx": 10.0, "dy": -5.0}),
                json!({"kind": "resize_object", "page": 0, "index": 0, "rect": rect(60.0, 700.0, 300.0, 40.0)}),
                json!({"kind": "delete_object", "page": 1, "index": 0}),
            ],
        ),
        (
            "retouche de texte",
            "simple.pdf",
            vec![json!({"kind": "set_object_text", "page": 0, "index": 0, "text": "Retouché"})],
        ),
        (
            "caviardage",
            "out-of-order.pdf",
            vec![json!({"kind": "redact", "page": 0, "rect": rect(60.0, 600.0, 300.0, 60.0)})],
        ),
        (
            "caviardage d'image",
            "image-mosaic.pdf",
            vec![json!({"kind": "redact", "page": 0, "rect": rect(150.0, 400.0, 100.0, 100.0)})],
        ),
        (
            "signature visuelle",
            "simple.pdf",
            vec![
                json!({"kind": "insert_signature", "page": 0, "rect": rect(350.0, 100.0, 150.0, 50.0),
                        "source": {"kind": "strokes", "strokes": [[[360.0, 110.0], [400.0, 130.0], [450.0, 115.0]]]}}),
            ],
        ),
        (
            "pièces jointes",
            "attachments.pdf",
            vec![
                json!({"kind": "add_attachment", "path": attached.to_string_lossy()}),
                json!({"kind": "delete_attachment", "index": 0}),
            ],
        ),
        (
            "formulaire rempli",
            "form.pdf",
            vec![
                json!({"kind": "set_field_value", "page": 0, "index": 0, "value": {"kind": "text", "text": "Dupont é"}}),
            ],
        ),
        (
            "formulaire aplati",
            "form.pdf",
            vec![json!({"kind": "flatten_form"})],
        ),
    ];
    for (what, source, commands) in cases {
        check(&qpdf, source, what, &commands);
    }
}

/// La couche de texte invisible de l'OCR (sans Tesseract : les mots sont fournis).
#[test]
fn an_ocr_text_layer_produces_a_conforming_file() {
    let Some(qpdf) = qpdf() else { return };
    let dir = tempfile::tempdir().unwrap();
    let mut doc = Document::open(fixture("scanned.pdf"), None).unwrap();
    doc.add_invisible_words(
        0,
        &[mega_core::overlay::InvisibleWord {
            text: "Reconnu".to_string(),
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
    assert_conforms(&qpdf, &out, None, "couche OCR");
}

/// Signature numérique (mise à jour incrémentale) sur un fichier intact et sur un fichier
/// modifié.
#[test]
fn a_digital_signature_produces_a_conforming_file() {
    let Some(qpdf) = qpdf() else { return };
    let dir = tempfile::tempdir().unwrap();
    let p12 = std::fs::read(fixture("test-signer.p12")).unwrap();
    let identity =
        mega_core::digital_signature::SigningIdentity::from_pkcs12(&p12, "test").unwrap();
    let options = mega_core::digital_signature::SignOptions {
        reason: Some("Contrôle".to_string()),
        location: None,
        contact: None,
        appearance: None,
    };
    for (what, modify) in [
        ("signature d'un fichier intact", false),
        ("signature d'un fichier modifié", true),
    ] {
        let mut doc = Document::open(fixture("simple.pdf"), None).unwrap();
        if modify {
            let rotate: EditCommand = serde_json::from_value(
                serde_json::json!({"kind": "rotate_pages", "pages": [0], "quarter_turns": 1}),
            )
            .unwrap();
            doc.apply(&rotate).unwrap();
        }
        let out = dir.path().join(format!("signe-{modify}.pdf"));
        doc.sign_to(&out, &identity, &options).unwrap();
        assert_conforms(&qpdf, &out, None, what);
    }
}

/// Garde-fou du contrôle lui-même : un fichier abîmé du corpus (objet sans `endobj`) est
/// bien refusé, sinon les tests ci-dessus ne prouveraient rien.
#[test]
#[should_panic(expected = "qpdf --check a échoué")]
fn the_check_rejects_a_damaged_file() {
    let Some(qpdf) = qpdf() else {
        panic!("qpdf --check a échoué (qpdf absent : garde-fou sans objet)");
    };
    assert_conforms(
        &qpdf,
        &fixture("robustesse/sans-endobj.pdf"),
        None,
        "corpus",
    );
}

/// Un fichier abîmé ou inhabituel, une fois modifié et enregistré par nous, devient un fichier
/// **sain** : la réécriture répare au lieu de recopier les défauts.
#[test]
fn damaged_inputs_are_written_back_as_conforming_files() {
    let Some(qpdf) = qpdf() else { return };
    use serde_json::json;
    for name in [
        "robustesse/prefixe-parasite.pdf",
        "robustesse/xref-decalee.pdf",
        "robustesse/sans-endobj.pdf",
        "robustesse/pdf-2-0.pdf",
        "robustesse/xref-hybride.pdf",
        "robustesse/revisions.pdf",
        "robustesse/linearise.pdf",
    ] {
        check(
            &qpdf,
            name,
            name,
            &[json!({"kind": "rotate_pages", "pages": [0], "quarter_turns": 1})],
        );
    }
}
