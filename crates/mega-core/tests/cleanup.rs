// SPDX-License-Identifier: GPL-3.0-or-later
//! Passe `lopdf` à l'enregistrement : `/InkList` de nos formes, `/AP` des notes, élagage.

use mega_core::testing::{identity_encrypted_note_pdf, sample_pdf};
use mega_core::{AnnotationKind, Color, CoreError, Document, Geometry, NewAnnotation, Rect, Style};

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
    let out = mega_core::cleanup::before_save(&bytes, None, None)
        .unwrap()
        .bytes;
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
    let out = mega_core::cleanup::before_save(&raw, None, None)
        .unwrap()
        .bytes;
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
    let out = mega_core::cleanup::before_save(&bytes, None, None)
        .unwrap()
        .bytes;
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
    let out = mega_core::cleanup::before_save(&doc.to_bytes().unwrap(), None, None)
        .unwrap()
        .bytes;
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
    let out = mega_core::cleanup::before_save(&doc.to_bytes().unwrap(), None, None)
        .unwrap()
        .bytes;
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
        mega_core::cleanup::before_save(&mega_core::testing::ink_with_matrix_pdf(), None, None)
            .unwrap()
            .bytes;
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

/// Mineur du Lot 9 : une annotation membre d'un **groupe** (`/IRT` + `/RT /Group`, §12.5.6.2)
/// disparaît avec l'annotation principale du groupe.
#[test]
fn group_members_go_with_their_primary_annotation() {
    let mut lo = lopdf::Document::load_mem(&sample_pdf(1, "groupe")).unwrap();
    let page_id = *lo.get_pages().get(&1).unwrap();
    let square = |rect: [i64; 4]| {
        lopdf::Dictionary::from_iter(vec![
            ("Type", lopdf::Object::Name(b"Annot".to_vec())),
            ("Subtype", lopdf::Object::Name(b"Square".to_vec())),
            (
                "Rect",
                lopdf::Object::Array(rect.iter().map(|v| lopdf::Object::Integer(*v)).collect()),
            ),
            (
                "C",
                lopdf::Object::Array(vec![1.into(), 0.into(), 0.into()]),
            ),
        ])
    };
    let primary = lo.add_object(square([100, 600, 200, 700]));
    let mut member = square([250, 600, 350, 700]);
    member.set("IRT", lopdf::Object::Reference(primary));
    member.set("RT", lopdf::Object::Name(b"Group".to_vec()));
    let member = lo.add_object(member);
    lo.get_dictionary_mut(page_id).unwrap().set(
        "Annots",
        lopdf::Object::Array(vec![primary.into(), member.into()]),
    );
    let mut bytes = Vec::new();
    lo.save_to(&mut bytes).unwrap();
    let mut doc = Document::from_bytes(bytes, None).unwrap();
    assert_eq!(doc.annotations(0).unwrap().len(), 2);
    doc.delete_annotation(0, 0).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("groupe.pdf");
    doc.save_as(&path).unwrap();
    let reopened = Document::open(&path, None).unwrap();
    assert!(
        reopened.annotations(0).unwrap().is_empty(),
        "membre du groupe parti aussi"
    );
}

/// `identity_encrypted_note_pdf` rendu à un chiffrement RC4 dont le filtre `/StdCF` annonce une
/// méthode (`/CFM /Vx`) que lopdf ne connaît pas : PDFium la lit comme du RC4 et ouvre le
/// fichier, lopdf n'en tire aucun filtre et la passe de nettoyage ne peut pas s'appliquer. Même
/// longueur que l'original (espaces de bourrage) : les décalages du fichier restent exacts.
fn uncleanable_note_pdf(secret: &str) -> Vec<u8> {
    let mut bytes = identity_encrypted_note_pdf(secret);
    let swap = |bytes: &mut Vec<u8>, from: &[u8], to: &[u8]| {
        assert_eq!(from.len(), to.len());
        let at = bytes.windows(from.len()).position(|w| w == from).unwrap();
        bytes[at..at + from.len()].copy_from_slice(to);
    };
    swap(
        &mut bytes,
        b"/StmF /Identity /StrF /Identity",
        b"/StmF /StdCF    /StrF /StdCF   ",
    );
    swap(&mut bytes, b"/CFM /V2", b"/CFM /Vx");
    bytes
}

#[test]
fn deleting_an_annotation_on_an_uncleanable_file_is_refused() {
    let mut doc = Document::from_bytes(uncleanable_note_pdf("SECRET-ZX9"), None).unwrap();
    let err = doc.delete_annotation(0, 0).unwrap_err();
    assert!(
        matches!(&err, CoreError::InvalidOperation(m) if m.contains("suppression est refusée"))
    );
    // La cause du refus est gardée (ADR-0005, revue du Lot 14, M2), sans préfixe répété.
    let message = err.to_string();
    assert!(message.contains("(document chiffré"), "{message}");
    assert_eq!(
        message.matches("Opération impossible").count(),
        1,
        "{message}"
    );
    // `is_modified` suit l'historique, pas le drapeau `changed` : `save_as` recopie les octets
    // d'origine tels quels tant que rien n'a changé.
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("sortie.pdf");
    doc.save_as(&out).unwrap();
    assert_eq!(
        std::fs::read(&out).unwrap(),
        uncleanable_note_pdf("SECRET-ZX9")
    );
}

#[test]
fn adding_an_annotation_on_an_uncleanable_file_still_works() {
    let mut doc = Document::from_bytes(uncleanable_note_pdf("x"), None).unwrap();
    let note = NewAnnotation {
        kind: AnnotationKind::Rect,
        geometry: Geometry::Rect {
            rect: Rect {
                x: 100.0,
                y: 100.0,
                w: 50.0,
                h: 50.0,
            },
        },
        style: style(),
        contents: String::new(),
        author: String::new(),
    };
    assert!(doc.add_annotation(0, &note).is_ok());
}

/// Opération de document essayée par les tests de garde (le résultat n'importe pas, seule
/// l'erreur compte).
type Attempt = fn(&mut Document) -> Result<(), CoreError>;

fn zone() -> Rect {
    Rect {
        x: 10.0,
        y: 10.0,
        w: 200.0,
        h: 200.0,
    }
}

/// Revue du Lot 14 (I1) : **toute** opération qui retire une information de l'affichage ou du
/// fichier est refusée sur un fichier que le nettoyage ne saura pas traiter, avant de toucher
/// au document (l'enregistrement recopie alors les octets d'origine).
#[test]
fn every_removing_operation_is_refused_on_an_uncleanable_file() {
    let guarded: [(&str, Attempt); 8] = [
        ("delete_annotation", |d| d.delete_annotation(0, 0)),
        ("update_annotation", |d| {
            let patch = mega_core::AnnotationPatch {
                contents: Some("autre".into()),
                ..Default::default()
            };
            d.update_annotation(0, 0, &patch).map(drop)
        }),
        ("set_annotation_rect", |d| {
            d.set_annotation_rect(0, 0, zone()).map(drop)
        }),
        ("delete_pages", |d| d.delete_pages(&[0])),
        ("delete_object", |d| d.delete_object(0, 0)),
        ("set_object_text", |d| d.set_object_text(0, 0, "x")),
        ("delete_attachment", |d| d.delete_attachment(0)),
        ("redact", |d| d.redact(0, zone()).map(drop)),
    ];
    let input = uncleanable_note_pdf("SECRET-ZX9");
    let dir = tempfile::tempdir().unwrap();
    for (name, attempt) in guarded {
        let mut doc = Document::from_bytes(input.clone(), None).unwrap();
        let err = attempt(&mut doc).unwrap_err();
        assert!(
            matches!(&err, CoreError::InvalidOperation(m) if m.contains("suppression est refusée")),
            "{name} : {err}"
        );
        let out = dir.path().join(format!("{name}.pdf"));
        doc.save_as(&out).unwrap();
        assert!(
            std::fs::read(&out).unwrap() == input,
            "{name} : document touché"
        );
    }
}

/// Déplacer ou redimensionner un objet ne retire rien : pas de refus de nettoyage (l'opération
/// peut échouer pour une autre raison, le contenu chiffré de cette page étant illisible).
#[test]
fn moving_or_resizing_an_object_is_not_refused_on_an_uncleanable_file() {
    let free: [(&str, Attempt); 2] = [
        ("move_object", |d| d.move_object(0, 0, 1.0, 1.0)),
        ("resize_object", |d| d.resize_object(0, 0, zone())),
    ];
    for (name, attempt) in free {
        let mut doc = Document::from_bytes(uncleanable_note_pdf("x"), None).unwrap();
        if let Err(err) = attempt(&mut doc) {
            assert!(
                !err.to_string().contains("suppression est refusée"),
                "{name} : {err}"
            );
        }
    }
}

/// Revue du Lot 14 (C1, I1) : caviarder, retirer une pièce jointe ou remplacer un texte
/// supprime quelque chose ; l'enregistrement devra vérifier que le nettoyage a eu lieu.
#[test]
fn removing_operations_set_needs_cleanup_on_a_normal_file() {
    let mut doc = Document::from_bytes(mega_core::testing::rich_pdf(), None).unwrap();
    assert!(!doc.needs_cleanup());
    doc.redact(
        0,
        Rect {
            x: 60.0,
            y: 690.0,
            w: 200.0,
            h: 40.0,
        },
    )
    .unwrap();
    assert!(doc.needs_cleanup(), "redact");

    let mut doc = Document::from_bytes(mega_core::testing::attachments_pdf(), None).unwrap();
    doc.delete_attachment(0).unwrap();
    assert!(doc.needs_cleanup(), "delete_attachment");

    let mut doc = Document::from_bytes(sample_pdf(1, "objets"), None).unwrap();
    doc.set_object_text(0, 0, "Bonjour").unwrap();
    assert!(doc.needs_cleanup(), "set_object_text");
}

/// Le passe-plat `Identity` et un fichier ordinaire se nettoient ; un filtre inconnu de lopdf,
/// non.
#[test]
fn can_clean_accepts_identity_pass_through_and_plain_files_but_not_unknown_filters() {
    assert!(mega_core::cleanup::can_clean(
        &identity_encrypted_note_pdf("x"),
        None
    ));
    assert!(!mega_core::cleanup::can_clean(
        &uncleanable_note_pdf("x"),
        None
    ));
    assert!(mega_core::cleanup::can_clean(&sample_pdf(1, "t"), None));
}

/// Revue du Lot 14 (M13) : après une suppression, `save_as` d'un fichier que le nettoyage
/// traite réussit, et le drapeau retombe.
#[test]
fn saving_a_cleanable_file_after_a_deletion_succeeds() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/simple.pdf");
    let mut doc = Document::open(path, None).unwrap();
    doc.delete_pages(&[1]).unwrap();
    assert!(doc.needs_cleanup());
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("sans-p2.pdf");
    doc.save_as(&out).unwrap();
    assert!(!doc.needs_cleanup());
    assert_eq!(Document::open(&out, None).unwrap().page_count(), 2);
}

/// `sample_pdf` réécrit par `lopdf` avec l'en-tête `version` (et, si demandé, `/Version` du
/// catalogue) : PDFium, lui, ne sait écrire que 1.0 à 1.7.
fn sample_with_header(version: &str, catalog_version: Option<&str>) -> Vec<u8> {
    let mut doc = lopdf::Document::load_mem(&sample_pdf(3, "t")).unwrap();
    doc.version = version.into();
    if let Some(v) = catalog_version {
        let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let catalog = doc.get_object_mut(root).unwrap().as_dict_mut().unwrap();
        catalog.set("Version", lopdf::Object::Name(v.as_bytes().to_vec()));
    }
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

#[test]
fn before_save_keeps_a_2_0_header() {
    let input = sample_with_header("2.0", None);
    let doc = Document::from_bytes(input, None).unwrap();
    let pdfium_bytes = doc.to_bytes().unwrap();
    assert!(
        pdfium_bytes.starts_with(b"%PDF-1."),
        "PDFium rabaisse l'en-tête"
    );
    let out = mega_core::cleanup::before_save(&pdfium_bytes, None, Some((2, 0)))
        .unwrap()
        .bytes;
    assert!(out.starts_with(b"%PDF-2.0"));
    let kept = mega_core::cleanup::before_save(&pdfium_bytes, None, None)
        .unwrap()
        .bytes;
    assert!(!kept.starts_with(b"%PDF-2.0"));
}

#[test]
fn before_save_raw_fallback_also_raises_the_header() {
    // Octets que lopdf ne relit pas : le repli brut recopie PDFium, version comprise.
    let junk = b"%PDF-1.4\nnot a pdf at all\n%%EOF\n";
    let out = mega_core::cleanup::before_save(junk, None, Some((2, 0)))
        .unwrap()
        .bytes;
    assert!(out.starts_with(b"%PDF-2.0"));
    assert_eq!(out.len(), junk.len());
}

/// Audit E1 : un fichier `/StmF /StrF /Identity` se relit par la session « passe-plat » ; la
/// note supprimée n'y survit donc pas, et le `/Encrypt` d'origine reste dans le fichier.
#[test]
fn deleted_note_does_not_survive_in_an_identity_encrypted_file() {
    let mut doc = Document::from_bytes(identity_encrypted_note_pdf("SECRET-ZX9"), None).unwrap();
    doc.delete_annotation(0, 0).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("o.pdf");
    doc.save_as(&out).unwrap();
    let bytes = std::fs::read(&out).unwrap();
    assert!(
        !bytes.windows(10).any(|w| w == b"SECRET-ZX9"),
        "le texte de la note supprimée survit"
    );
    assert!(
        bytes.windows(9).any(|w| w == b"/Encrypt "),
        "/Encrypt d'origine conservé"
    );
    assert!(
        !bytes.windows(8).any(|w| w == b"/Encrypx"),
        "aucune clé rebaptisée écrite"
    );
    let reopened = Document::from_bytes(bytes, None).unwrap();
    assert!(reopened.page_text(0).unwrap().contains("Visible"));
}

/// La passe dit si elle a pu s'appliquer : `Document` refuse d'enregistrer une suppression
/// dont le nettoyage a dû être sauté (second repli de `before_save`, audit E1).
#[test]
fn before_save_reports_whether_the_cleanup_applied() {
    let cleaned = mega_core::cleanup::before_save(&sample_pdf(1, "t"), None, None).unwrap();
    assert!(cleaned.cleaned);
    let input = uncleanable_note_pdf("x");
    let fallback = mega_core::cleanup::before_save(&input, None, None).unwrap();
    assert!(!fallback.cleaned, "filtre inconnu de lopdf : repli");
    assert_eq!(fallback.bytes, input, "le repli rend les octets tels quels");
}

// --- Destinations orphelines après suppression de pages (Lot 14, audit E4) -------------------

#[test]
fn bookmarks_to_a_deleted_page_lose_their_destination_but_remain() {
    let dir = tempfile::tempdir().unwrap();
    let mut doc =
        Document::from_bytes(mega_core::testing::rich_catalog_pdf("1.7", None), None).unwrap();
    doc.delete_pages(&[1]).unwrap();
    let path = dir.path().join("sans-p2.pdf");
    doc.save_as(&path).unwrap();
    let saved = std::fs::read(&path).unwrap();
    let reopened = Document::from_bytes(saved.clone(), None).unwrap();
    let outline: Vec<(String, Option<u16>)> = reopened
        .outline()
        .unwrap()
        .into_iter()
        .map(|n| (n.title, n.page))
        .collect();
    assert_eq!(
        outline,
        [
            ("Vers chap2".to_string(), None),
            ("Vers chap3".to_string(), Some(1)),
            ("Direct p3".to_string(), Some(1)),
        ]
    );
    // Le lien vers la page supprimée est retiré ; les deux autres restent.
    assert_eq!(reopened.links(0).unwrap().len(), 2);
    // Plus rien ne garde la page supprimée dans le fichier.
    let lo = lopdf::Document::load_mem(&saved).unwrap();
    let in_tree: std::collections::BTreeSet<lopdf::ObjectId> =
        lo.get_pages().values().copied().collect();
    let ghosts: Vec<lopdf::ObjectId> = lo
        .objects
        .iter()
        .filter(|(id, object)| {
            !in_tree.contains(id)
                && object.as_dict().is_ok_and(|d| {
                    d.get(b"Type").and_then(lopdf::Object::as_name).ok() == Some(b"Page".as_slice())
                })
        })
        .map(|(id, _)| *id)
        .collect();
    assert!(ghosts.is_empty(), "pages hors de l'arbre : {ghosts:?}");
    // PDFium efface l'objet de la page supprimée : ce qui le visait pointe vers `null`. Le
    // fichier ne doit plus le citer (signet sans `/Dest`, nom retiré, lien retiré).
    let text = |o: &lopdf::Object| String::from_utf8_lossy(o.as_str().unwrap()).into_owned();
    let bookmark = lo
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .find(|d| d.get(b"Title").is_ok_and(|t| text(t) == "Vers chap2"))
        .expect("signet gardé");
    assert!(!bookmark.has(b"Dest") && !bookmark.has(b"A"));
    let raw = String::from_utf8_lossy(&saved);
    assert!(!raw.contains("(chap2)"), "destination nommée retirée");
    let page = lo.get_pages()[&1];
    let links = lo
        .get_page_annotations(page)
        .unwrap()
        .into_iter()
        .filter(|a| {
            a.get(b"Subtype").and_then(lopdf::Object::as_name).ok() == Some(b"Link".as_slice())
        })
        .count();
    assert_eq!(links, 2, "lien vers la page supprimée retiré de /Annots");
}

/// Entiers cités par le `/K` de chaque élément de structure `/P`, dans l'ordre des objets.
fn struct_elem_mcids(bytes: &[u8]) -> Vec<Vec<i64>> {
    let doc = lopdf::Document::load_mem(bytes).unwrap();
    doc.objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .filter(|d| d.get(b"Type").and_then(lopdf::Object::as_name).ok() == Some(b"StructElem"))
        .map(|d| match d.get(b"K") {
            Ok(lopdf::Object::Integer(n)) => vec![*n],
            Ok(lopdf::Object::Array(a)) => a.iter().filter_map(|k| k.as_i64().ok()).collect(),
            _ => vec![],
        })
        .collect()
}

#[test]
fn deleting_one_of_two_marked_objects_prunes_its_mcid_from_the_structure_tree() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("balise.pdf");
    let mut doc = Document::from_bytes(mega_core::testing::two_marked_objects_pdf(), None).unwrap();
    doc.delete_object(0, 0).unwrap();
    doc.save_as(&path).unwrap();
    let out = std::fs::read(&path).unwrap();
    let mut mcids = struct_elem_mcids(&out);
    mcids.sort();
    // L'élément du texte supprimé n'a plus de `/K` ; l'autre cite toujours son MCID.
    assert_eq!(mcids.concat(), vec![1], "mcids restants : {mcids:?}");
    let reread = lopdf::Document::load_mem(&out).unwrap();
    let alts = reread
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .filter(|d| d.has(b"Alt"))
        .count();
    assert_eq!(alts, 1, "l'`/Alt` de l'élément vidé est retiré");
    // Le `/ParentTree` n'annonce plus l'élément supprimé : entrée 0 nulle, entrée 1 intacte.
    let tree = reread
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .find(|d| d.has(b"Nums"))
        .unwrap();
    let nums = tree.get(b"Nums").unwrap().as_array().unwrap();
    let entries = reread
        .get_object(nums[1].as_reference().unwrap())
        .unwrap()
        .as_array()
        .unwrap();
    assert!(matches!(entries[0], lopdf::Object::Null), "{entries:?}");
    assert!(entries[1].as_reference().is_ok(), "{entries:?}");
}

/// Relecture finale du Lot 14 (I3, constat F24-1) : un fichier à filtres `Identity` dont le mot
/// de passe d'ouverture n'est pas vide s'ouvre sans mot de passe (PDFium n'a rien à
/// déchiffrer) ; `lopdf` ne s'y authentifie pas, mais la session passe-plat n'en a pas besoin.
/// La suppression doit donc réussir, et le texte supprimé disparaître du fichier.
#[test]
fn deleted_note_does_not_survive_in_an_identity_file_with_a_user_password() {
    let input = mega_core::testing::identity_encrypted_note_with_user_password_pdf("SECRET-I3U");
    let mut doc = Document::from_bytes(input, None).unwrap();
    assert!(doc.security().unwrap().encrypted);
    doc.delete_annotation(0, 0).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("o.pdf");
    doc.save_as(&out).unwrap();
    let bytes = std::fs::read(&out).unwrap();
    assert!(
        !bytes.windows(10).any(|w| w == b"SECRET-I3U"),
        "le texte de la note supprimée survit"
    );
    assert!(
        bytes.windows(9).any(|w| w == b"/Encrypt "),
        "/Encrypt d'origine conservé"
    );
    let reopened = Document::from_bytes(bytes, None).unwrap();
    assert!(reopened.security().unwrap().encrypted);
    assert!(
        reopened
            .annotations(0)
            .unwrap()
            .iter()
            .all(|a| a.contents.as_deref() != Some("SECRET-I3U"))
    );
}
