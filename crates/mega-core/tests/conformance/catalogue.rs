// SPDX-License-Identifier: GPL-3.0-or-later
//! Les 17 structures du catalogue survivent aux éditions (audit E1 à E5) ; la fusion,
//! l'extraction et la scission reportent ce que `pages/carry.rs` promet, et rien d'autre.
//!
//! Le fichier de départ est `testing::rich_catalog_pdf` : trois pages et un catalogue qui
//! porte les 17 structures mesurées par le banc de l'audit (`CATALOG_PATHS`). Sans ce test,
//! une édition qui réécrit le document (PDFium recopie, `lopdf` élague) pouvait perdre un
//! formulaire, des signets ou des pièces jointes sans qu'aucun contrôle ne le voie : qpdf juge
//! la syntaxe, pas ce qui manque.

use std::path::{Path, PathBuf};

use lopdf::{Document as LoDocument, LoadOptions, Object};
use mega_core::Document;
use mega_core::testing::rich_catalog_pdf;
use serde_json::{Value, json};

use crate::{
    Case, annotation, apply_and_save, assert_conforms, missing_variants, output_password, qpdf,
    rect,
};

/// Les 17 structures du catalogue, chacune comme un chemin de clés depuis `/Root` (liste du
/// banc d'audit, `measure_catalog`).
const CATALOG_PATHS: [&[&str]; 17] = [
    &["StructTreeRoot"],
    &["MarkInfo"],
    &["OCProperties"],
    &["Names", "Dests"],
    &["Names", "JavaScript"],
    &["Names", "EmbeddedFiles"],
    &["Dests"],
    &["PageLabels"],
    &["Lang"],
    &["OpenAction"],
    &["AA"],
    &["ViewerPreferences"],
    &["PageMode"],
    &["Metadata"],
    &["OutputIntents"],
    &["Outlines"],
    &["AcroForm"],
];

/// Lecture `lopdf` d'un fichier produit, déchiffré avec `password`, sous le même plafond de
/// décompression que le crate (`crypt::load_limited`, 256 Mio).
fn read(bytes: &[u8], password: Option<&str>) -> LoDocument {
    let options = LoadOptions {
        password: password.map(str::to_string),
        max_decompressed_size: Some(256 << 20),
        ..LoadOptions::default()
    };
    LoDocument::load_mem_with_options(bytes, options).unwrap()
}

/// Vrai si `path` mène, depuis le catalogue, à un objet (chaque étape déréférencée).
fn has_path(doc: &LoDocument, path: &[&str]) -> bool {
    let Ok(catalog) = doc.catalog() else {
        return false;
    };
    let mut current = Object::Dictionary(catalog.clone());
    for key in path {
        let Ok(dict) = current.as_dict() else {
            return false;
        };
        let Ok(value) = dict.get(key.as_bytes()) else {
            return false;
        };
        let Ok((_, value)) = doc.dereference(value) else {
            return false;
        };
        current = value.clone();
    }
    true
}

/// Les structures de `CATALOG_PATHS` présentes dans `bytes`, sous la forme `Names/Dests`.
fn structures(bytes: &[u8], password: Option<&str>) -> Vec<String> {
    let doc = read(bytes, password);
    CATALOG_PATHS
        .iter()
        .filter(|path| has_path(&doc, path))
        .map(|path| path.join("/"))
        .collect()
}

/// Une commande par variante d'`EditCommand` (hors `ocr_page`), appliquée au catalogue riche
/// `source`. Les index visent son contenu : quatre annotations sur la page 1 (trois liens, le
/// widget `nom`, index 3), un objet texte par page, la pièce jointe `notes.txt`. Les objets
/// touchés sont ceux de la page 3, hors de l'arbre de structure.
fn rich_cases(source: &Path, dir: &Path) -> Vec<Case> {
    let image = dir.join("image.png");
    image::RgbImage::from_pixel(40, 20, image::Rgb([10, 120, 200]))
        .save(&image)
        .unwrap();
    let image = image.to_string_lossy().into_owned();
    let attached = dir.join("note.txt");
    std::fs::write(&attached, "pièce jointe").unwrap();
    let attached = attached.to_string_lossy().into_owned();
    let protect = json!({"kind": "protect", "protection": {"user_password": "u",
        "owner_password": "o", "permissions": {"print": true, "copy": false, "modify": false,
        "annotate": true}}});
    let simple = crate::fixture("simple.pdf").to_string_lossy().into_owned();
    let square = annotation(
        "rect",
        json!({"kind": "rect", "rect": rect(300.0, 300.0, 100.0, 50.0)}),
    );
    let cases: Vec<(&'static str, Vec<Value>)> = vec![
        (
            "rotate_pages",
            vec![json!({"kind": "rotate_pages", "pages": [0], "quarter_turns": 1})],
        ),
        (
            "delete_pages",
            vec![json!({"kind": "delete_pages", "pages": [2]})],
        ),
        (
            "move_pages",
            vec![json!({"kind": "move_pages", "pages": [0], "to": 2})],
        ),
        (
            "insert_blank_page",
            vec![json!({"kind": "insert_blank_page", "at": 1,
                        "size": {"width_pt": 595.0, "height_pt": 842.0}})],
        ),
        (
            "insert_pages_from_file",
            vec![json!({"kind": "insert_pages_from_file", "path": simple, "pages": [0], "at": 3})],
        ),
        ("add_annotation", vec![square.clone()]),
        (
            "delete_annotation",
            vec![json!({"kind": "delete_annotation", "page": 0, "index": 2})],
        ),
        (
            "update_annotation",
            // Les liens ne se modifient pas : la retouche vise une annotation ajoutée (index 4).
            vec![
                square.clone(),
                json!({"kind": "update_annotation", "page": 0, "index": 4,
                        "patch": {"contents": "modifiée"}}),
            ],
        ),
        (
            "move_annotation",
            vec![
                square.clone(),
                json!({"kind": "move_annotation", "page": 0, "index": 4,
                        "rect": rect(72.0, 400.0, 128.0, 20.0)}),
            ],
        ),
        (
            "set_field_value",
            vec![json!({"kind": "set_field_value", "page": 0, "index": 3,
                        "value": {"kind": "text", "text": "Dupont"}})],
        ),
        ("flatten_form", vec![json!({"kind": "flatten_form"})]),
        (
            "insert_signature",
            vec![
                json!({"kind": "insert_signature", "page": 0, "rect": rect(350.0, 100.0, 150.0, 50.0),
                        "source": {"kind": "strokes", "strokes": [[[360.0, 110.0], [400.0, 130.0]]]}}),
            ],
        ),
        (
            "delete_object",
            vec![json!({"kind": "delete_object", "page": 2, "index": 0})],
        ),
        (
            "move_object",
            vec![json!({"kind": "move_object", "page": 2, "index": 0, "dx": 10.0, "dy": -5.0})],
        ),
        (
            "resize_object",
            vec![json!({"kind": "resize_object", "page": 2, "index": 0,
                        "rect": rect(72.0, 700.0, 200.0, 40.0)})],
        ),
        (
            "set_object_text",
            vec![json!({"kind": "set_object_text", "page": 2, "index": 0, "text": "Page trois"})],
        ),
        (
            "insert_text",
            vec![
                json!({"kind": "insert_text", "page": 0, "at": [100.0, 300.0], "text": "Ajout",
                        "style": {"family": "helvetica", "bold": false, "italic": false,
                                  "size": 12.0, "color": {"r": 0, "g": 0, "b": 0}}}),
            ],
        ),
        (
            "insert_image",
            vec![
                json!({"kind": "insert_image", "page": 0, "rect": rect(300.0, 200.0, 80.0, 40.0),
                        "path": image}),
            ],
        ),
        (
            "redact",
            vec![json!({"kind": "redact", "page": 2, "rect": rect(60.0, 740.0, 200.0, 50.0)})],
        ),
        (
            "watermark",
            vec![
                json!({"kind": "watermark", "spec": {"text": "BROUILLON", "size": 40.0,
                        "opacity": 0.3, "angle_deg": 45.0, "color": {"r": 200, "g": 0, "b": 0},
                        "pages": null}}),
            ],
        ),
        (
            "page_numbers",
            vec![
                json!({"kind": "page_numbers", "spec": {"template": "{n} / {total}",
                        "corner": "bottom_center", "margin": 20.0, "size": 10.0, "start": 1,
                        "pages": null}}),
            ],
        ),
        (
            "crop_pages",
            vec![json!({"kind": "crop_pages", "spec": {"pages": [0],
                        "margins": [10.0, 10.0, 10.0, 10.0]}})],
        ),
        (
            "set_metadata",
            vec![json!({"kind": "set_metadata", "patch": {"title": "Riche retitré"}})],
        ),
        ("protect", vec![protect.clone()]),
        (
            "unprotect",
            vec![protect, json!({"kind": "unprotect", "password": null})],
        ),
        (
            "optimize",
            vec![json!({"kind": "optimize", "options": {"image_quality": 75,
                        "max_image_px": null, "compress_streams": true}})],
        ),
        (
            "add_attachment",
            vec![json!({"kind": "add_attachment", "path": attached})],
        ),
        (
            // Ajoutée puis retirée (`note.txt` précède `notes.txt` dans l'ordre des clés) :
            // `notes.txt` reste, l'arbre `/EmbeddedFiles` aussi.
            "delete_attachment",
            vec![
                json!({"kind": "add_attachment", "path": attached}),
                json!({"kind": "delete_attachment", "index": 0}),
            ],
        ),
    ];
    cases
        .into_iter()
        .map(|(what, commands)| Case {
            what,
            source: source.to_path_buf(),
            password: None,
            commands,
        })
        .collect()
}

/// Ce qu'une édition retire **exprès** du catalogue riche : aplatir le formulaire supprime
/// `/AcroForm` (ses champs sont devenus du contenu de page). Toute autre perte est un défaut.
fn removed_on_purpose(what: &str) -> &'static [&'static str] {
    match what {
        "flatten_form" => &["AcroForm"],
        _ => &[],
    }
}

/// Chaque variante d'`EditCommand` garde les 17 structures du catalogue riche après
/// `save_as` (sauf ce que [`removed_on_purpose`] annonce). Les sorties passent aussi qpdf
/// quand il est là. Tous les manques sont listés avant l'assertion.
#[test]
fn every_edit_command_keeps_the_rich_catalog() {
    let qpdf = qpdf();
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("riche.pdf");
    let rich = rich_catalog_pdf("1.7", None);
    std::fs::write(&source, &rich).unwrap();
    assert_eq!(
        structures(&rich, None).len(),
        17,
        "le générateur porte les 17 structures"
    );
    let cases = rich_cases(&source, dir.path());
    let missing = missing_variants(cases.iter().flat_map(|c| c.commands.iter()));
    assert!(missing.is_empty(), "variantes sans cas : {missing:?}");
    let mut failures = Vec::new();
    for case in &cases {
        let case_dir = tempfile::tempdir().unwrap();
        let out: PathBuf = apply_and_save(case, case_dir.path());
        let password = output_password(case);
        if let Some(qpdf) = &qpdf {
            assert_conforms(qpdf, &out, password.as_deref(), case.what);
        }
        let kept = structures(&std::fs::read(&out).unwrap(), password.as_deref());
        let lost: Vec<String> = CATALOG_PATHS
            .iter()
            .map(|path| path.join("/"))
            .filter(|path| !kept.contains(path))
            .filter(|path| !removed_on_purpose(case.what).contains(&path.as_str()))
            .collect();
        if !lost.is_empty() {
            failures.push(format!("{} : perdu {lost:?}", case.what));
        }
        for path in removed_on_purpose(case.what) {
            if kept.iter().any(|k| k == path) {
                failures.push(format!("{} : {path} aurait dû disparaître", case.what));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Ce que la fusion, l'extraction et la scission ne reportent **jamais** (`pages/carry.rs`) :
/// les scripts (`/Names /JavaScript`, `/OpenAction`, `/AA` : on n'importe pas de code) et
/// l'arbre de structure, qui ne se découpe pas page par page (avec lui `/MarkInfo`, qui
/// déclarerait balisé un document qui ne l'est plus).
const NEVER_CARRIED: [&str; 5] = [
    "StructTreeRoot",
    "MarkInfo",
    "Names/JavaScript",
    "OpenAction",
    "AA",
];

/// Ce que `carry_simple` reporte quelles que soient les pages copiées.
const ALWAYS_CARRIED: [&str; 7] = [
    "Names/EmbeddedFiles",
    "PageLabels",
    "Lang",
    "ViewerPreferences",
    "PageMode",
    "Metadata",
    "OutputIntents",
];

/// Fusion, extraction et scission du catalogue riche : chaque sortie porte ce que
/// `pages/carry.rs` promet pour les pages copiées, jamais ce qu'il refuse de reporter.
/// Selon les pages : `/OCProperties` suit la page 2 (qui utilise le calque), `/Names /Dests`
/// (`chap2`) la page 2, `/Dests` (`chap3`) la page 3, `/AcroForm` la page 1 (son widget), et
/// `/Outlines` les pages 2 et 3 (visées par les signets ; la page 1 ne l'est pas).
#[test]
fn merge_extract_and_split_carry_the_promised_catalog() {
    use mega_core::pages::{merge, split};
    let rich = Document::from_bytes(rich_catalog_pdf("1.7", None), None).unwrap();
    // (description, octets produits, pages de la source copiées)
    let mut outputs: Vec<(String, Vec<u8>, Vec<u16>)> = vec![
        (
            "fusion riche + riche".into(),
            merge(&[&rich, &rich]).unwrap(),
            vec![0, 1, 2],
        ),
        (
            "extraction p2".into(),
            rich.extract_pages(&[1]).unwrap(),
            vec![1],
        ),
        (
            "extraction p3, p1".into(),
            rich.extract_pages(&[2, 0]).unwrap(),
            vec![2, 0],
        ),
    ];
    let parts = [vec![0], vec![1, 2]];
    for (i, bytes) in split(&rich, &parts).unwrap().into_iter().enumerate() {
        outputs.push((
            format!("scission, partie {}", i + 1),
            bytes,
            parts[i].clone(),
        ));
    }
    let mut failures = Vec::new();
    for (what, bytes, pages) in outputs {
        let kept = structures(&bytes, None);
        let mut promised: Vec<&str> = ALWAYS_CARRIED.to_vec();
        if pages.contains(&1) || pages.contains(&2) {
            promised.push("Outlines");
        }
        if pages.contains(&0) {
            promised.push("AcroForm");
        }
        if pages.contains(&1) {
            promised.extend(["OCProperties", "Names/Dests"]);
        }
        if pages.contains(&2) {
            promised.push("Dests");
        }
        for path in promised {
            if !kept.iter().any(|k| k == path) {
                failures.push(format!("{what} : {path} non reporté"));
            }
        }
        for path in NEVER_CARRIED {
            if kept.iter().any(|k| k == path) {
                failures.push(format!("{what} : {path} reporté (interdit)"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
