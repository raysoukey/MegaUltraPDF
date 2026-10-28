// SPDX-License-Identifier: GPL-3.0-or-later
//! Passe `lopdf` à l'enregistrement : `/InkList` de nos formes, `/AP` des notes, élagage.

use mega_core::testing::sample_pdf;
use mega_core::{AnnotationKind, Color, Document, Geometry, NewAnnotation, Rect, Style};

fn style() -> Style {
    Style {
        color: Color {
            r: 229,
            g: 57,
            b: 53,
        },
        fill: None,
        opacity: 1.0,
        stroke_width: 2.0,
        font_size: 12.0,
    }
}

fn count_objects(bytes: &[u8]) -> usize {
    let text = String::from_utf8_lossy(bytes);
    text.matches(" 0 obj").count()
}

#[test]
fn save_cleanup_strips_note_ap_and_adds_inklist() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("propre.pdf");
    std::fs::write(&path, sample_pdf(1, "cleanup")).unwrap();
    let mut doc = Document::open(&path, None).unwrap();
    doc.add_annotation(
        0,
        &NewAnnotation {
            kind: AnnotationKind::Note,
            geometry: Geometry::Rect {
                rect: Rect {
                    x: 50.0,
                    y: 700.0,
                    w: 0.0,
                    h: 0.0,
                },
            },
            style: style(),
            contents: "Bonjour".to_string(),
            author: "Moi".to_string(),
        },
    )
    .unwrap();
    doc.add_annotation(
        0,
        &NewAnnotation {
            kind: AnnotationKind::Rect,
            geometry: Geometry::Rect {
                rect: Rect {
                    x: 100.0,
                    y: 500.0,
                    w: 200.0,
                    h: 100.0,
                },
            },
            style: style(),
            contents: String::new(),
            author: String::new(),
        },
    )
    .unwrap();
    for _ in 0..5 {
        doc.render_page(0, 0.5).unwrap();
    }
    let raw = doc.to_bytes().unwrap();
    assert!(String::from_utf8_lossy(&raw).contains("PDFIUM_HasGeneratedAP"));
    let before = doc.render_page(0, 1.0).unwrap();
    doc.save().unwrap();
    let saved = std::fs::read(&path).unwrap();
    let text = String::from_utf8_lossy(&saved);
    assert!(
        !text.contains("PDFIUM_HasGeneratedAP"),
        "clé privée retirée"
    );
    assert!(
        text.contains("/InkList"),
        "/InkList écrit pour le rectangle"
    );
    assert!(
        count_objects(&saved) < count_objects(&raw),
        "objets orphelins élagués"
    );
    let reopened = Document::open(&path, None).unwrap();
    let anns = reopened.annotations(0).unwrap();
    assert_eq!(anns.len(), 2);
    assert_eq!(anns[0].kind, AnnotationKind::Note);
    assert_eq!(anns[0].contents.as_deref(), Some("Bonjour"));
    assert_eq!(anns[1].kind, AnnotationKind::Rect);
    assert_eq!(reopened.render_page(0, 1.0).unwrap().rgba, before.rgba);
}

/// Sans le mot de passe d'ouverture, un document chiffré ne peut pas être déchiffré : il est
/// écrit tel que PDFium l'a produit (avec le mot de passe : voir `tests/encrypted.rs`).
#[test]
fn cleanup_without_password_keeps_encrypted_bytes() {
    let bytes = std::fs::read(format!(
        "{}/../../fixtures/encrypted.pdf",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let out = mega_core::cleanup::before_save(&bytes, None).unwrap();
    assert_eq!(out, bytes);
}

#[test]
fn cleanup_inklist_follows_ink_strokes() {
    let mut doc = Document::from_bytes(sample_pdf(1, "ink"), None).unwrap();
    doc.add_annotation(
        0,
        &NewAnnotation {
            kind: AnnotationKind::Ink,
            geometry: Geometry::Strokes {
                strokes: vec![vec![[10.0, 10.0], [20.0, 30.0], [40.0, 25.0]]],
            },
            style: style(),
            contents: String::new(),
            author: String::new(),
        },
    )
    .unwrap();
    let raw = doc.to_bytes().unwrap();
    let out = mega_core::cleanup::before_save(&raw, None).unwrap();
    let text = String::from_utf8_lossy(&out);
    let i = text.find("/InkList").expect("InkList");
    let tail = &text[i..i + 80];
    assert!(
        tail.contains("10") && tail.contains("30") && tail.contains("40"),
        "{tail}"
    );
}

/// La clé privée `/PDFIUM_HasGeneratedAP` est posée aussi sur les marquages de texte : elle
/// disparaît du fichier (leur apparence, elle, est conservée : elle est portable).
#[test]
fn save_cleanup_strips_private_key_from_markups_but_keeps_their_appearance() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("marquage.pdf");
    std::fs::write(&path, sample_pdf(1, "marquage")).unwrap();
    let mut doc = Document::open(&path, None).unwrap();
    doc.add_annotation(
        0,
        &NewAnnotation {
            kind: AnnotationKind::Highlight,
            geometry: Geometry::Quads {
                rects: vec![Rect {
                    x: 70.0,
                    y: 750.0,
                    w: 100.0,
                    h: 24.0,
                }],
            },
            style: style(),
            contents: String::new(),
            author: String::new(),
        },
    )
    .unwrap();
    doc.render_page(0, 0.5).unwrap();
    assert!(String::from_utf8_lossy(&doc.to_bytes().unwrap()).contains("PDFIUM_HasGeneratedAP"));
    let before = doc.render_page(0, 1.0).unwrap();
    doc.save().unwrap();
    let saved = std::fs::read(&path).unwrap();
    assert!(!String::from_utf8_lossy(&saved).contains("PDFIUM_HasGeneratedAP"));
    let reopened = Document::open(&path, None).unwrap();
    assert_eq!(reopened.render_page(0, 1.0).unwrap().rgba, before.rgba);
}

fn fixture_bytes(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// B1 (pdf-expert) : un PDF protégé par un mot de passe **propriétaire** seul s'ouvre sans mot
/// de passe ; `lopdf` le déchiffre alors en silence et `is_encrypted()` ment. L'enregistrement
/// doit conserver le chiffrement, et les passes `lopdf` le rechiffrer (Lot 10).
#[test]
fn owner_password_document_keeps_its_encryption_on_save() {
    let bytes = fixture_bytes("owner-only.pdf");
    assert!(String::from_utf8_lossy(&bytes).contains("/Encrypt"));
    let out = mega_core::cleanup::before_save(&bytes, None).unwrap();
    assert!(
        String::from_utf8_lossy(&out).contains("/Encrypt"),
        "la passe de nettoyage ne doit pas déchiffrer"
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("owner.pdf");
    std::fs::write(&path, &bytes).unwrap();
    let mut doc = Document::open(&path, None).unwrap();
    doc.rotate_pages(&[0], 1).unwrap();
    doc.save().unwrap();
    let saved = std::fs::read(&path).unwrap();
    assert!(
        String::from_utf8_lossy(&saved).contains("/Encrypt"),
        "toujours chiffré"
    );
    let reopened = Document::open(&path, None).unwrap();
    assert_eq!(reopened.page_count(), 3);
    // Lot 10 : les passes qui réécrivent par lopdf rechiffrent au lieu de refuser.
    let bytes = reopened_watermarked(&path);
    assert!(
        String::from_utf8_lossy(&bytes).contains("/Encrypt"),
        "filigrané et toujours chiffré"
    );
}

fn reopened_watermarked(path: &std::path::Path) -> Vec<u8> {
    let mut doc = Document::open(path, None).unwrap();
    let spec = mega_core::overlay::Watermark {
        text: "X".to_string(),
        size: 40.0,
        opacity: 0.5,
        angle_deg: 0.0,
        color: Color { r: 0, g: 0, b: 0 },
        pages: None,
    };
    doc.add_watermark(&spec).unwrap();
    assert!(doc.page_text(0).unwrap().contains('X'));
    doc.to_bytes().unwrap()
}

/// A5 (pdf-expert) : deux superpositions successives ne doivent pas emboîter deux enveloppes
/// `q … Q` (limite de profondeur d'Acrobat) : la nôtre est reconnue et réutilisée.
#[test]
fn second_overlay_reuses_the_existing_wrapper() {
    let mut doc = Document::from_bytes(sample_pdf(1, "double"), None).unwrap();
    let spec = mega_core::overlay::Watermark {
        text: "UN".to_string(),
        size: 40.0,
        opacity: 0.5,
        angle_deg: 0.0,
        color: Color { r: 0, g: 0, b: 0 },
        pages: None,
    };
    doc.add_watermark(&spec).unwrap();
    doc.add_watermark(&mega_core::overlay::Watermark {
        text: "DEUX".to_string(),
        ..spec
    })
    .unwrap();
    let lo = lopdf::Document::load_mem(&doc.to_bytes().unwrap()).unwrap();
    let page = lo.get_pages()[&1];
    let streams = lo.get_page_contents(page);
    // [q, contenu d'origine, Q, UN, DEUX] : cinq flux, pas sept.
    assert_eq!(streams.len(), 5, "{streams:?}");
    let text: String = doc.text_boxes(0).unwrap().iter().map(|b| b.ch).collect();
    assert!(text.contains("UN") && text.contains("DEUX"));
}

// --- Lot 9 : /NM et /P (B3), courbes de l'/InkList (B25) ---------------------------------

/// Les annotations de la page 1, avec la référence de la page.
fn annots_of(doc: &lopdf::Document) -> Vec<(lopdf::ObjectId, lopdf::Dictionary)> {
    let page = *doc.get_pages().get(&1).unwrap();
    let annots = doc.get_dictionary(page).unwrap().get(b"Annots").unwrap();
    let array = match annots {
        lopdf::Object::Reference(id) => doc.get_object(*id).unwrap().as_array().unwrap().clone(),
        lopdf::Object::Array(a) => a.clone(),
        _ => panic!("/Annots"),
    };
    array
        .iter()
        .map(|o| match o {
            lopdf::Object::Reference(id) => (page, doc.get_dictionary(*id).unwrap().clone()),
            lopdf::Object::Dictionary(d) => (page, d.clone()),
            _ => panic!("annotation"),
        })
        .collect()
}

fn shape(kind: AnnotationKind, x: f32, y: f32, w: f32, h: f32) -> NewAnnotation {
    NewAnnotation {
        kind,
        geometry: Geometry::Rect {
            rect: Rect { x, y, w, h },
        },
        style: style(),
        contents: String::new(),
        author: String::new(),
    }
}

#[test]
fn saved_annotations_have_nm_and_p() {
    let mut doc = Document::from_bytes(sample_pdf(1, "nm"), None).unwrap();
    for x in [50.0, 200.0] {
        doc.add_annotation(0, &shape(AnnotationKind::Rect, x, 100.0, 80.0, 40.0))
            .unwrap();
    }
    let out = mega_core::cleanup::before_save(&doc.to_bytes().unwrap(), None).unwrap();
    let saved = lopdf::Document::load_mem(&out).unwrap();
    let annots = annots_of(&saved);
    assert_eq!(annots.len(), 2);
    let mut names = std::collections::BTreeSet::new();
    for (page, dict) in annots {
        let nm = dict.get(b"NM").unwrap().as_str().unwrap().to_vec();
        assert!(names.insert(nm), "/NM unique");
        assert_eq!(dict.get(b"P").unwrap().as_reference().unwrap(), page);
    }
}

/// Nos ellipses sont des `Ink` dessinées en courbes de Bézier : l'`/InkList` doit suivre la
/// courbe (points échantillonnés), pas seulement ses quatre extrémités (un losange).
#[test]
fn ellipse_ink_list_samples_the_curves() {
    let mut doc = Document::from_bytes(sample_pdf(1, "ellipse"), None).unwrap();
    doc.add_annotation(
        0,
        &shape(AnnotationKind::Ellipse, 100.0, 100.0, 200.0, 100.0),
    )
    .unwrap();
    let out = mega_core::cleanup::before_save(&doc.to_bytes().unwrap(), None).unwrap();
    let saved = lopdf::Document::load_mem(&out).unwrap();
    let (_, dict) = annots_of(&saved).remove(0);
    let points: Vec<f32> = dict
        .get(b"InkList")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|s| s.as_array().unwrap().iter().map(|v| v.as_float().unwrap()))
        .collect();
    assert!(points.len() / 2 >= 16, "{} points", points.len() / 2);
    let xs: Vec<f32> = points.iter().step_by(2).copied().collect();
    let ys: Vec<f32> = points.iter().skip(1).step_by(2).copied().collect();
    let min = |v: &[f32]| v.iter().copied().fold(f32::MAX, f32::min);
    let max = |v: &[f32]| v.iter().copied().fold(f32::MIN, f32::max);
    let (cx, rx) = ((min(&xs) + max(&xs)) / 2.0, (max(&xs) - min(&xs)) / 2.0);
    let (cy, ry) = ((min(&ys) + max(&ys)) / 2.0, (max(&ys) - min(&ys)) / 2.0);
    for (x, y) in xs.iter().zip(&ys) {
        let e = ((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2);
        assert!(
            (e - 1.0).abs() < 0.05,
            "point ({x}, {y}) hors de l'ellipse : {e}"
        );
    }
}

/// Une apparence avec `/Matrix` : les points de l'`/InkList` sont dans l'espace de la page.
#[test]
fn ink_matrix_is_applied() {
    let out =
        mega_core::cleanup::before_save(&mega_core::testing::ink_with_matrix_pdf(), None).unwrap();
    let saved = lopdf::Document::load_mem(&out).unwrap();
    let (_, dict) = annots_of(&saved).remove(0);
    let first: Vec<f32> = dict.get(b"InkList").unwrap().as_array().unwrap()[0]
        .as_array()
        .unwrap()
        .iter()
        .take(2)
        .map(|v| v.as_float().unwrap())
        .collect();
    assert_eq!(first, vec![110.0, 120.0]);
}
