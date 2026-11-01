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
    // Une variable vide compte comme absente.
    let exe = std::env::var_os("MEGA_QPDF")
        .filter(|v| !v.is_empty())
        .map_or_else(|| PathBuf::from("qpdf"), PathBuf::from);
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

/// Fusion, scission et extraction assemblent un document neuf, puis y reportent le catalogue
/// des sources (Lot 14, audit E3) : formulaire, signets, pièces jointes, étiquettes, calques.
#[test]
fn merge_split_and_extract_produce_conforming_files() {
    let Some(qpdf) = qpdf() else { return };
    use mega_core::pages::{merge, split};
    use mega_core::testing;
    let dir = tempfile::tempdir().unwrap();
    let open = |bytes: Vec<u8>| Document::from_bytes(bytes, None).unwrap();
    let rich = open(testing::rich_catalog_pdf("1.7", None));
    let form = open(testing::form_pdf());
    let parent_kid = open(testing::form_parent_kid_pdf());
    let simple = Document::open(fixture("simple.pdf"), None).unwrap();
    let mut outputs: Vec<(String, Vec<u8>)> = vec![
        (
            "fusion riche + riche".into(),
            merge(&[&rich, &rich]).unwrap(),
        ),
        (
            "fusion formulaire + formulaire".into(),
            merge(&[&form, &form]).unwrap(),
        ),
        (
            "fusion simple + parent/enfant".into(),
            merge(&[&simple, &parent_kid]).unwrap(),
        ),
        ("extraction p2".into(), rich.extract_pages(&[1]).unwrap()),
        (
            "extraction p3, p1".into(),
            rich.extract_pages(&[2, 0]).unwrap(),
        ),
    ];
    for (i, part) in split(&rich, &[vec![0], vec![1, 2]])
        .unwrap()
        .into_iter()
        .enumerate()
    {
        outputs.push((format!("scission, partie {}", i + 1), part));
    }
    for (what, bytes) in outputs {
        let path = dir.path().join("sortie.pdf");
        std::fs::write(&path, bytes).unwrap();
        assert_conforms(&qpdf, &path, None, &what);
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

/// C2 (audit du Lot 13) : signer un fichier à structure inhabituelle (références hybrides,
/// révisions successives, linéarisé, table réparée) donne une signature **intacte** à la
/// relecture et un fichier que qpdf accepte.
#[test]
fn signing_unusual_structures_gives_intact_conforming_signatures() {
    let qpdf = qpdf();
    let dir = tempfile::tempdir().unwrap();
    let p12 = std::fs::read(fixture("test-signer.p12")).unwrap();
    let identity =
        mega_core::digital_signature::SigningIdentity::from_pkcs12(&p12, "test").unwrap();
    let options = mega_core::digital_signature::SignOptions {
        reason: None,
        location: None,
        contact: None,
        appearance: None,
    };
    for name in [
        "robustesse/xref-hybride.pdf",
        "robustesse/revisions.pdf",
        "robustesse/linearise.pdf",
        "robustesse/xref-decalee.pdf",
        "robustesse/sans-endobj.pdf",
        "robustesse/prefixe-parasite.pdf",
        "robustesse/pdf-2-0.pdf",
    ] {
        let mut doc = Document::open(fixture(name), None).unwrap();
        let text_before = doc.page_text(0).unwrap();
        let out = dir.path().join("signe.pdf");
        doc.sign_to(&out, &identity, &options)
            .unwrap_or_else(|e| panic!("{name} : signature refusée : {e}"));
        let signed = Document::open(&out, None).unwrap();
        let list = signed.signatures().unwrap();
        assert_eq!(list.len(), 1, "{name} : {list:?}");
        assert!(
            matches!(
                list[0].status,
                mega_core::digital_signature::SignatureStatus::Intact
            ),
            "{name} : {:?}",
            list[0].status
        );
        assert_eq!(
            signed.page_text(0).unwrap(),
            text_before,
            "{name} : contenu changé"
        );
        if let Some(qpdf) = &qpdf {
            assert_conforms(qpdf, &out, None, name);
        }
    }
}

/// Déplacer une page d'un catalogue riche réordonne l'arbre des pages (`/Kids`) ; supprimer
/// une page neutralise les destinations qui la visaient (Lot 14, audit E4).
#[test]
fn moving_and_deleting_rich_pages_produce_conforming_files() {
    let Some(qpdf) = qpdf() else { return };
    use serde_json::json;
    let dir = tempfile::tempdir().unwrap();
    for (what, json) in [
        (
            "déplacer p1 en fin",
            json!({"kind": "move_pages", "pages": [0], "to": 2}),
        ),
        (
            "supprimer p2",
            json!({"kind": "delete_pages", "pages": [1]}),
        ),
    ] {
        let mut doc =
            Document::from_bytes(mega_core::testing::rich_catalog_pdf("1.7", None), None).unwrap();
        let command: EditCommand = serde_json::from_value(json).unwrap();
        doc.apply(&command).unwrap();
        let out = dir.path().join("sortie.pdf");
        doc.save_as(&out).unwrap();
        assert_conforms(&qpdf, &out, None, what);
    }
}

/// Déplacer des pages **d'un nœud `/Pages` à l'autre** (`two_level_pages_pdf`) : chaque sortie
/// passe `qpdf --check`, et chaque page garde la taille et l'orientation qu'elle avait, alors
/// qu'elle quitte les attributs hérités de son ancien nœud (audit E4).
#[test]
fn moving_pages_across_page_tree_nodes_produces_conforming_files() {
    let Some(qpdf) = qpdf() else { return };
    let dir = tempfile::tempdir().unwrap();
    let source = mega_core::testing::two_level_pages_pdf();
    let original = Document::from_bytes(source.clone(), None).unwrap();
    let before: Vec<_> = (0..4)
        .map(|i| {
            (
                original.page_rotation(i).unwrap(),
                original.page_size(i).unwrap(),
            )
        })
        .collect();
    let cases: [(&str, &[u16], u16); 4] = [
        ("page 1 vers la fin (nœud A → B)", &[0], 3),
        ("page 4 en tête (nœud B → A)", &[3], 0),
        ("sélection [1, 3] croisant les nœuds", &[0, 2], 2),
        ("nœud A vidé", &[0, 1], 2),
    ];
    for (what, pages, to) in cases {
        let mut doc = Document::from_bytes(source.clone(), None).unwrap();
        let json = serde_json::json!({"kind": "move_pages", "pages": pages, "to": to});
        let command: EditCommand = serde_json::from_value(json).unwrap();
        doc.apply(&command).unwrap();
        let out = dir.path().join("sortie.pdf");
        doc.save_as(&out).unwrap();
        assert_conforms(&qpdf, &out, None, what);
        // Ordre attendu : les pages restantes, le groupe déplacé inséré à `to`.
        let mut order: Vec<u16> = (0..4).filter(|i| !pages.contains(i)).collect();
        let at = usize::from(to);
        order.splice(at..at, pages.iter().copied());
        let reopened = Document::open(&out, None).unwrap();
        assert_eq!(reopened.page_count(), 4, "{what}");
        for (position, &origin) in order.iter().enumerate() {
            let position = position as u16;
            let text = reopened.page_text(position).unwrap();
            assert!(
                text.contains(&format!("Page {}", origin + 1)),
                "{what} : page {position} = « {text} »"
            );
            let after = (
                reopened.page_rotation(position).unwrap(),
                reopened.page_size(position).unwrap(),
            );
            assert_eq!(
                after,
                before[usize::from(origin)],
                "{what} : rotation et taille de la page {}",
                origin + 1
            );
        }
    }
}

/// veraPDF et le corpus, s'ils sont fournis (`MEGA_VERAPDF` : script `verapdf` ou `.bat` ;
/// `MEGA_CORPUS` : dossier `fixtures/corpus`). Sans eux, les tests veraPDF sont sautés.
fn verapdf_setup() -> Option<(PathBuf, PathBuf)> {
    let verapdf = std::env::var_os("MEGA_VERAPDF").filter(|v| !v.is_empty())?;
    let corpus = std::env::var_os("MEGA_CORPUS").filter(|v| !v.is_empty())?;
    Some((PathBuf::from(verapdf), PathBuf::from(corpus)))
}

/// Rapport veraPDF (`--format text`) d'un fichier pour un profil, et son verdict : vrai si la
/// sortie commence par `PASS`.
fn verapdf_report(verapdf: &Path, flavour: &str, file: &Path) -> (bool, String) {
    let report = Command::new(verapdf)
        .args(["--flavour", flavour, "--format", "text", "--verbose"])
        .arg(file)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&report.stdout).into_owned();
    (text.starts_with("PASS"), text)
}

/// Profil veraPDF d'un fichier du corpus : `0` (déduit de la déclaration XMP) pour PDF/A,
/// `ua1` / `ua2` d'après le nom pour PDF/UA.
fn flavour_of(name: &str) -> &'static str {
    if name.starts_with("pdfua-1") {
        "ua1"
    } else if name.starts_with("pdfua-2") {
        "ua2"
    } else {
        "0"
    }
}

/// Le filigrane du contrôle : translucide (0,3), pour exercer l'opacité et la transparence.
fn draft_watermark() -> mega_core::overlay::Watermark {
    mega_core::overlay::Watermark {
        text: "BROUILLON".to_string(),
        size: 40.0,
        opacity: 0.3,
        angle_deg: 45.0,
        color: mega_core::Color {
            r: 200,
            g: 30,
            b: 30,
        },
        pages: None,
    }
}

/// Un filigrane et une numérotation ne cassent la conformité d'**aucun** fichier conforme du
/// corpus : PDF/UA (artefacts), PDF/A-1 (ni transparence ni opacité), PDF/A-2 à 4 (couleur
/// indépendante du périphérique, groupe de transparence) — audit E6. Les échecs sont tous
/// listés avant l'assertion, pour voir d'un coup ce qui reste à corriger.
#[test]
fn watermark_and_page_numbers_keep_the_conformance_of_every_corpus_file() {
    let Some((verapdf, corpus)) = verapdf_setup() else {
        eprintln!("MEGA_VERAPDF / MEGA_CORPUS absents : contrôle veraPDF sauté");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();
    let mut checked = 0;
    for folder in ["pdfa", "pdfua"] {
        let mut files: Vec<PathBuf> = std::fs::read_dir(corpus.join(folder))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "pdf"))
            .collect();
        files.sort();
        for source in files {
            let name = source.file_name().unwrap().to_string_lossy().into_owned();
            let mut doc = Document::open(&source, None).unwrap();
            doc.add_watermark(&draft_watermark()).unwrap();
            doc.add_page_numbers(&mega_core::overlay::PageNumbering {
                template: "Page {n} / {total}".to_string(),
                corner: mega_core::overlay::Corner::BottomCenter,
                margin: 20.0,
                size: 10.0,
                start: 1,
                pages: None,
            })
            .unwrap();
            let out = dir.path().join(&name);
            doc.save_as(&out).unwrap();
            let flavour = flavour_of(&name);
            let (pass, report) = verapdf_report(&verapdf, flavour, &out);
            checked += 1;
            if !pass {
                failures.push(format!(
                    "{name} (veraPDF {flavour})
{report}"
                ));
            }
        }
    }
    assert!(checked > 0, "corpus vide");
    assert!(
        failures.is_empty(),
        "{} fichier(s) sur {checked} ne passent plus :
{}",
        failures.len(),
        failures.join(
            "
"
        )
    );
}

/// Changer le titre d'un fichier PDF/A-4 n'écrit que le XMP : PDF/A-4 interdit `/Info`
/// (6.1.3-t4/t5), hors `/ModDate` existant (audit E6).
#[test]
fn retitling_a_pdfa4_file_keeps_its_conformance() {
    let Some((verapdf, corpus)) = verapdf_setup() else {
        eprintln!("MEGA_VERAPDF / MEGA_CORPUS absents : contrôle veraPDF sauté");
        return;
    };
    let source = corpus.join("pdfa").join("pdfa-4-6-1-2-t01-pass-a.pdf");
    let mut doc = Document::open(&source, None).unwrap();
    doc.set_metadata(&mega_core::properties::MetadataPatch {
        title: Some("Nouveau titre".to_string()),
        author: Some("Ève".to_string()),
        ..Default::default()
    })
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("titre.pdf");
    doc.save_as(&out).unwrap();
    let (pass, report) = verapdf_report(&verapdf, "0", &out);
    assert!(
        pass,
        "PDF/A-4 retitré : veraPDF
{report}"
    );
}
