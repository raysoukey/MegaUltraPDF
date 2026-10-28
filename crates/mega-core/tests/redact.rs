// SPDX-License-Identifier: GPL-3.0-or-later
//! Caviardage réel : le contenu caviardé disparaît du fichier, pas seulement de l'écran.

use mega_core::edit::EditCommand;
use mega_core::edit::EditCommand as Cmd;
use mega_core::testing::{form_pdf, rich_pdf, rotated_pdf, xobject_text_pdf};
use mega_core::{CoreError, Document, ObjectKind, Rect};

fn rich() -> Document {
    Document::from_bytes(rich_pdf(), None).unwrap()
}

fn page_text(doc: &Document, page: u16) -> String {
    doc.text_boxes(page)
        .unwrap()
        .iter()
        .map(|b| b.ch)
        .filter(|c| !c.is_whitespace())
        .collect()
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, w, h }
}

fn pixel(img: &mega_core::RenderedPage, x: u32, y: u32) -> (u8, u8, u8) {
    let i = ((y * img.width_px + x) * 4) as usize;
    (img.rgba[i], img.rgba[i + 1], img.rgba[i + 2])
}

#[test]
fn rich_fixture_has_text_image_path_and_annotation() {
    let doc = rich();
    assert_eq!(page_text(&doc, 0), "Secret123Public");
    let kinds: Vec<ObjectKind> = doc
        .page_objects(0)
        .unwrap()
        .iter()
        .map(|o| o.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            ObjectKind::Text,
            ObjectKind::Text,
            ObjectKind::Image,
            ObjectKind::Path
        ]
    );
    assert_eq!(doc.annotations(0).unwrap().len(), 1);
}

#[test]
fn redaction_removes_text_from_extraction() {
    let mut doc = rich();
    let zone = rect(60.0, 690.0, 200.0, 40.0);
    let report = doc.redact(0, zone).unwrap();
    assert_eq!(
        (report.texts, report.images_removed, report.annotations),
        (1, 0, 1)
    );
    let text = page_text(&doc, 0);
    assert!(!text.contains("Secret") && !text.contains("123"), "{text}");
    assert!(text.contains("Public"), "{text}");
    assert!(
        doc.annotations(0).unwrap().is_empty(),
        "le carré qui couvrait le secret"
    );
    // Un rectangle noir recouvre la zone.
    let img = doc.render_page(0, 1.0).unwrap();
    assert_eq!(pixel(&img, 160, 842 - 710), (0, 0, 0));
    // Le fichier réenregistré ne contient plus le texte.
    let reopened = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    assert_eq!(page_text(&reopened, 0), "Public");
    assert_eq!(
        pixel(&reopened.render_page(0, 1.0).unwrap(), 160, 842 - 710),
        (0, 0, 0)
    );
    let raw = reopened.to_bytes().unwrap();
    assert!(
        !String::from_utf8_lossy(&raw).contains("Secret 123"),
        "la chaîne ne doit plus apparaître en clair dans le fichier"
    );
}

#[test]
fn redaction_is_an_undoable_command() {
    let mut doc = rich();
    let before = doc.render_page(0, 0.5).unwrap();
    doc.apply(&EditCommand::Redact {
        page: 0,
        rect: rect(60.0, 690.0, 200.0, 40.0),
    })
    .unwrap();
    assert_eq!(doc.undo_label().as_deref(), Some("Caviarder"));
    assert_eq!(page_text(&doc, 0), "Public");
    doc.undo().unwrap();
    assert_eq!(page_text(&doc, 0), "Secret123Public");
    assert_eq!(doc.render_page(0, 0.5).unwrap().rgba, before.rgba);
}

#[test]
fn redaction_removes_images_and_annotations() {
    let mut doc = rich();
    // L'image est posée en (300, 680) sur 100 × 50 : la zone en couvre toute la hauteur.
    let report = doc.redact(0, rect(290.0, 670.0, 120.0, 80.0)).unwrap();
    assert_eq!(
        (report.texts, report.images_removed, report.annotations),
        (0, 1, 0)
    );
    let kinds: Vec<ObjectKind> = doc
        .page_objects(0)
        .unwrap()
        .iter()
        .map(|o| o.kind)
        .collect();
    // Les deux textes et le chemin restent ; l'image est partie ; le cache noir est un chemin.
    assert_eq!(
        kinds,
        vec![
            ObjectKind::Text,
            ObjectKind::Text,
            ObjectKind::Path,
            ObjectKind::Path
        ]
    );
    assert_eq!(page_text(&doc, 0), "Secret123Public");
    assert_eq!(doc.annotations(0).unwrap().len(), 1);
}

#[test]
fn redaction_outside_page_is_refused() {
    let mut doc = rich();
    for bad in [
        rect(-50.0, 100.0, 100.0, 100.0),
        rect(500.0, 100.0, 200.0, 100.0),
        rect(100.0, 100.0, 0.0, 100.0),
        rect(100.0, 100.0, f32::NAN, 100.0),
    ] {
        assert!(
            matches!(doc.redact(0, bad), Err(CoreError::InvalidOperation(_))),
            "{bad:?}"
        );
    }
    assert!(matches!(
        doc.redact(4, rect(100.0, 100.0, 10.0, 10.0)),
        Err(CoreError::PageOutOfRange { .. })
    ));
    assert_eq!(page_text(&doc, 0), "Secret123Public");
    assert_eq!(doc.page_objects(0).unwrap().len(), 4);
}

#[test]
fn redaction_on_rotated_page_uses_displayed_rect() {
    let mut doc = Document::from_bytes(rotated_pdf(), None).unwrap();
    // « Page 1 » s'affiche vers x 555..650, y 300..330 (voir tests/rotation.rs).
    let report = doc.redact(0, rect(540.0, 240.0, 55.0, 120.0)).unwrap();
    assert_eq!(report.texts, 1);
    assert_eq!(page_text(&doc, 0), "");
    let img = doc.render_page(0, 1.0).unwrap();
    assert_eq!(pixel(&img, 565, 400 - 300), (0, 0, 0));
    // En dehors de la zone, la page est restée blanche.
    assert_eq!(pixel(&img, 300, 200), (255, 255, 255));
}

/// C1 (relecture) : l'image caviardée ne doit plus être dans le **fichier**, pas seulement
/// hors du flux de contenu (son XObject restait référencé par `/Resources`).
#[test]
fn redaction_removes_image_and_annotation_bytes_from_saved_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("riche.pdf");
    std::fs::write(&path, rich_pdf()).unwrap();
    let mut doc = Document::open(&path, None).unwrap();
    // La zone couvre toute l'image (300..400 × 680..730) : elle est retirée entière.
    doc.redact(0, rect(290.0, 670.0, 120.0, 80.0)).unwrap();
    doc.redact(0, rect(60.0, 690.0, 200.0, 40.0)).unwrap();
    doc.save().unwrap();
    let saved = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into_owned();
    assert!(
        !saved.contains("/Image"),
        "le flux image a disparu du fichier"
    );
    assert!(!saved.contains("00FF FF00"), "ses pixels aussi");
    assert!(
        !saved.contains("/Square"),
        "l'annotation retirée a disparu du fichier"
    );
    assert!(!saved.contains("Secret 123"));
    let reopened = Document::open(&path, None).unwrap();
    assert_eq!(page_text(&reopened, 0), "Public");
    assert_eq!(
        pixel(&reopened.render_page(0, 1.0).unwrap(), 160, 842 - 710),
        (0, 0, 0)
    );
}

/// C2 (relecture) : du texte dessiné à travers un XObject resterait sous le cache noir ;
/// plutôt que de mentir, on refuse et le document reste intact.
#[test]
fn redaction_fails_when_text_remains_under_the_zone() {
    let mut doc = Document::from_bytes(xobject_text_pdf(), None).unwrap();
    assert_eq!(page_text(&doc, 0), "Cache");
    let before = doc.render_page(0, 0.5).unwrap();
    let result = doc.apply(&Cmd::Redact {
        page: 0,
        rect: rect(60.0, 690.0, 100.0, 40.0),
    });
    match result {
        Err(CoreError::InvalidOperation(m)) => assert!(m.contains("reste"), "{m}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(page_text(&doc, 0), "Cache");
    assert_eq!(
        doc.render_page(0, 0.5).unwrap().rgba,
        before.rgba,
        "pas de cache noir posé"
    );
    assert!(!doc.can_undo() && !doc.is_modified());
}

/// B2 (pdf-expert) : retirer le widget d'un champ laisse sa valeur dans `/AcroForm` ; on
/// refuse plutôt que de prétendre l'avoir caviardée.
#[test]
fn redaction_refuses_form_fields() {
    let mut doc = Document::from_bytes(form_pdf(), None).unwrap();
    let before = doc.form_fields(0).unwrap().len();
    // Le champ « ref » (REF-001) est en [72 460 200 484].
    match doc.apply(&Cmd::Redact {
        page: 0,
        rect: rect(60.0, 450.0, 160.0, 40.0),
    }) {
        Err(CoreError::InvalidOperation(m)) => assert!(m.contains("champ"), "{m}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(doc.form_fields(0).unwrap().len(), before);
    assert!(!doc.is_modified());
}

// --- Lot 9 (B20) : une note retirée ne survit ni par son popup, ni par une réponse, ni par
// l'arbre de structure.

/// Vrai si `needle` apparaît dans une chaîne ou un flux (décodé) de l'objet.
fn object_contains(object: &lopdf::Object, needle: &[u8]) -> bool {
    use lopdf::Object;
    let has = |bytes: &[u8]| bytes.windows(needle.len()).any(|w| w == needle);
    match object {
        Object::String(bytes, _) => has(bytes),
        Object::Array(items) => items.iter().any(|o| object_contains(o, needle)),
        Object::Dictionary(dict) => dict.iter().any(|(_, o)| object_contains(o, needle)),
        Object::Stream(stream) => {
            stream.dict.iter().any(|(_, o)| object_contains(o, needle))
                || has(&stream
                    .decompressed_content()
                    .unwrap_or_else(|_| stream.content.clone()))
        }
        _ => false,
    }
}

fn saved_contains(doc: &mut Document, needle: &str) -> bool {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("enregistre.pdf");
    doc.save_as(&path).unwrap();
    let saved = lopdf::Document::load_mem(&std::fs::read(&path).unwrap()).unwrap();
    saved
        .objects
        .values()
        .any(|o| object_contains(o, needle.as_bytes()))
}

#[test]
fn redacted_note_does_not_survive_through_its_popup() {
    let mut doc = Document::from_bytes(
        mega_core::testing::note_with_popup_pdf("NoteTresSecrete"),
        None,
    )
    .unwrap();
    assert!(saved_contains(&mut doc, "NoteTresSecrete"), "témoin");
    let mut doc = Document::from_bytes(
        mega_core::testing::note_with_popup_pdf("NoteTresSecrete"),
        None,
    )
    .unwrap();
    doc.redact(0, rect(40.0, 40.0, 40.0, 40.0)).unwrap();
    assert!(!saved_contains(&mut doc, "NoteTresSecrete"));
    // La réponse, hors zone, reste.
    assert!(saved_contains(&mut doc, "Reponse"));
    // Aucun marqueur `/OBJR` laissé sans sa cible `/Obj` (clé obligatoire, §14.7.4.3).
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("balise.pdf");
    doc.save_as(&path).unwrap();
    let saved = lopdf::Document::load_mem(&std::fs::read(&path).unwrap()).unwrap();
    fn objr_without_obj(o: &lopdf::Object) -> bool {
        match o {
            lopdf::Object::Dictionary(d) => {
                (d.get(b"Type").ok() == Some(&lopdf::Object::Name(b"OBJR".to_vec()))
                    && !d.has(b"Obj"))
                    || d.iter().any(|(_, v)| objr_without_obj(v))
            }
            lopdf::Object::Array(a) => a.iter().any(objr_without_obj),
            _ => false,
        }
    }
    assert!(
        !saved.objects.values().any(objr_without_obj),
        "marqueur /OBJR invalide"
    );
}

#[test]
fn deleting_a_note_also_drops_it_from_the_saved_file() {
    let mut doc = Document::from_bytes(
        mega_core::testing::note_with_popup_pdf("NoteTresSecrete"),
        None,
    )
    .unwrap();
    doc.apply(&EditCommand::DeleteAnnotation { page: 0, index: 0 })
        .unwrap();
    assert!(!saved_contains(&mut doc, "NoteTresSecrete"));
}

/// Relecture du Lot 9 : extraire (ou scinder) après un caviardage ne doit pas faire revenir la
/// note par sa réponse (`/IRT`) : les pages extraites passent par le même nettoyage.
#[test]
fn extracted_pages_do_not_carry_a_redacted_note() {
    let mut doc = Document::from_bytes(
        mega_core::testing::note_with_popup_pdf("NoteTresSecrete"),
        None,
    )
    .unwrap();
    doc.redact(0, rect(40.0, 40.0, 40.0, 40.0)).unwrap();
    let extracted = doc.extract_pages(&[0]).unwrap();
    let saved = lopdf::Document::load_mem(&extracted).unwrap();
    assert!(
        !saved
            .objects
            .values()
            .any(|o| object_contains(o, b"NoteTresSecrete")),
        "la note caviardée revient dans l'extrait"
    );
}

// --- Lot 10 : caviardage d'une partie d'image ---------------------------------------------

use mega_core::testing::{MOSAIC_SECRET, image_mosaic_pdf};

fn mosaic() -> Document {
    Document::from_bytes(image_mosaic_pdf(), None).unwrap()
}

/// Vrai si le motif secret (12 octets) apparaît dans un flux décompressé de `bytes`.
fn secret_in_file(bytes: &[u8]) -> bool {
    let needle = MOSAIC_SECRET.repeat(4);
    let doc = lopdf::Document::load_mem(bytes).unwrap();
    doc.objects.values().any(|o| {
        o.as_stream().is_ok_and(|s| {
            let data = s
                .decompressed_content()
                .unwrap_or_else(|_| s.content.clone());
            data.windows(needle.len()).any(|w| w == needle.as_slice())
        })
    })
}

/// Point d'une page de 842 pt de haut rendue à l'échelle 1 (espace affiché).
fn px(img: &mega_core::RenderedPage, x: f32, y: f32) -> (u8, u8, u8) {
    pixel(img, x as u32, img.height_px - y as u32)
}

#[test]
fn partial_image_redaction_blacks_only_the_zone() {
    let mut doc = mosaic();
    // Quadrant haut-gauche de l'image : x 100..200, y 200..300.
    let report = doc.redact(0, rect(100.0, 200.0, 100.0, 100.0)).unwrap();
    assert_eq!((report.images_redacted, report.images_removed), (1, 0));
    let img = doc.render_page(0, 1.0).unwrap();
    assert_eq!(px(&img, 150.0, 250.0), (0, 0, 0), "quadrant caviardé");
    assert_eq!(px(&img, 250.0, 250.0), (255, 0, 0), "rouge intact");
    assert_eq!(px(&img, 150.0, 150.0), (0, 128, 0), "vert intact");
    assert_eq!(px(&img, 250.0, 150.0), (0, 0, 255), "bleu intact");
    assert_eq!(
        doc.page_objects(0)
            .unwrap()
            .iter()
            .filter(|o| o.kind == ObjectKind::Image)
            .count(),
        2,
        "l'image reste (remplacée), l'autre aussi"
    );
}

#[test]
fn redacted_pixels_are_not_in_the_saved_file() {
    let mut doc = Document::from_bytes(image_mosaic_pdf(), None).unwrap();
    // Une seule page qui porte l'image : on retire les deux autres d'abord.
    doc.apply(&Cmd::DeletePages { pages: vec![1, 2] }).unwrap();
    assert!(
        secret_in_file(&doc.to_bytes().unwrap()),
        "motif présent avant"
    );
    doc.redact(0, rect(95.0, 195.0, 110.0, 110.0)).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("caviarde.pdf");
    doc.save_as(&path).unwrap();
    assert!(
        !secret_in_file(&std::fs::read(&path).unwrap()),
        "motif disparu du fichier"
    );
}

#[test]
fn shared_image_stays_intact_on_other_page() {
    let mut doc = mosaic();
    doc.redact(0, rect(100.0, 200.0, 100.0, 100.0)).unwrap();
    let other = doc.render_page(1, 1.0).unwrap();
    assert_eq!(
        px(&other, 150.0, 250.0),
        (0xC0, 0xFF, 0xEE),
        "page 1 intacte"
    );
}

#[test]
fn partial_redaction_on_rotated_page() {
    let mut doc = mosaic();
    // Boîte affichée de l'image sur la page tournée ; on caviarde son quart haut-gauche.
    let image = doc
        .page_objects(2)
        .unwrap()
        .into_iter()
        .find(|o| o.kind == ObjectKind::Image)
        .unwrap()
        .bounds;
    let zone = rect(
        image.x,
        image.y + image.h / 2.0,
        image.w / 2.0,
        image.h / 2.0,
    );
    let before = doc.render_page(2, 1.0).unwrap();
    let report = doc.redact(2, zone).unwrap();
    assert_eq!(report.images_redacted, 1);
    let img = doc.render_page(2, 1.0).unwrap();
    let (cx, cy) = (zone.x + zone.w / 2.0, zone.y + zone.h / 2.0);
    assert_eq!(px(&img, cx, cy), (0, 0, 0), "zone affichée noire");
    // Le quart opposé garde sa couleur d'avant.
    let (ox, oy) = (image.x + image.w * 0.75, image.y + image.h * 0.25);
    assert_eq!(px(&img, ox, oy), px(&before, ox, oy));
}

#[test]
fn smask_zone_becomes_opaque() {
    let mut doc = mosaic();
    // Moitié gauche de l'image masquée (transparente) : x 350..400, y 100..200.
    let before = doc.render_page(0, 1.0).unwrap();
    assert_eq!(
        px(&before, 360.0, 150.0),
        (255, 255, 255),
        "transparent avant"
    );
    doc.redact(0, rect(355.0, 120.0, 30.0, 30.0)).unwrap();
    let img = doc.render_page(0, 1.0).unwrap();
    assert_eq!(px(&img, 370.0, 135.0), (0, 0, 0), "zone noire et opaque");
    assert_eq!(px(&img, 390.0, 180.0), (255, 255, 255), "reste transparent");
    assert_eq!(px(&img, 430.0, 150.0), (0, 0, 255), "moitié opaque intacte");
}

// --- Lot 10 : tracés, vignettes, compte rendu --------------------------------------------

fn kinds(doc: &Document, page: u16) -> Vec<ObjectKind> {
    doc.page_objects(page)
        .unwrap()
        .iter()
        .map(|o| o.kind)
        .collect()
}

#[test]
fn path_inside_zone_is_removed() {
    let mut doc = rich();
    // Le rectangle bleu occupe 72..172 × 500..540.
    let report = doc.redact(0, rect(60.0, 490.0, 130.0, 60.0)).unwrap();
    assert_eq!((report.paths_removed, report.paths_kept), (1, 0));
    // Textes, image, puis le cache noir : le rectangle bleu a disparu.
    assert_eq!(
        kinds(&doc, 0),
        vec![
            ObjectKind::Text,
            ObjectKind::Text,
            ObjectKind::Image,
            ObjectKind::Path
        ]
    );
}

#[test]
fn crossing_path_is_counted_kept() {
    let mut doc = rich();
    let report = doc.redact(0, rect(100.0, 510.0, 30.0, 20.0)).unwrap();
    assert_eq!((report.paths_removed, report.paths_kept), (0, 1));
    assert_eq!(kinds(&doc, 0).len(), 5, "rectangle bleu gardé + cache");
}

#[test]
fn thumbnail_and_piece_info_are_dropped() {
    let mut lo = lopdf::Document::load_mem(&rich_pdf()).unwrap();
    let page_id = *lo.get_pages().get(&1).unwrap();
    let thumb = lo.add_object(lopdf::Stream::new(
        lopdf::Dictionary::from_iter(vec![
            ("Width", lopdf::Object::Integer(1)),
            ("Height", lopdf::Object::Integer(1)),
            ("ColorSpace", lopdf::Object::Name(b"DeviceGray".to_vec())),
            ("BitsPerComponent", lopdf::Object::Integer(8)),
        ]),
        vec![0x7F],
    ));
    let page = lo.get_dictionary_mut(page_id).unwrap();
    page.set("Thumb", lopdf::Object::Reference(thumb));
    page.set(
        "PieceInfo",
        lopdf::Object::Dictionary(lopdf::Dictionary::from_iter(vec![(
            "Editeur",
            lopdf::Object::Dictionary(lopdf::Dictionary::new()),
        )])),
    );
    let mut bytes = Vec::new();
    lo.save_to(&mut bytes).unwrap();
    let mut doc = Document::from_bytes(bytes, None).unwrap();
    doc.redact(0, rect(60.0, 690.0, 200.0, 40.0)).unwrap();
    let saved = lopdf::Document::load_mem(&doc.to_bytes().unwrap()).unwrap();
    let page = saved
        .get_dictionary(*saved.get_pages().get(&1).unwrap())
        .unwrap();
    assert!(!page.has(b"Thumb") && !page.has(b"PieceInfo"));
}

#[test]
fn redaction_report_is_in_edit_state() {
    let mut doc = rich();
    doc.apply(&Cmd::Redact {
        page: 0,
        rect: rect(60.0, 690.0, 200.0, 40.0),
    })
    .unwrap();
    let report = doc.edit_state().last_redaction.expect("compte rendu");
    assert_eq!((report.texts, report.annotations), (1, 1));
    doc.undo().unwrap();
    assert!(doc.edit_state().last_redaction.is_none());
}

// --- Relecture du Lot 10 : formulaires, images alternatives, motifs ----------------------

use lopdf::{Dictionary as LDict, Object as LObj, Stream as LStream};

/// Une image RGB 8 × 8 d'une couleur, éventuellement accompagnée de clés supplémentaires.
fn rgb_image(rgb: [u8; 3], extra: Vec<(&str, LObj)>) -> LStream {
    let mut d = LDict::from_iter(vec![
        ("Type", LObj::Name(b"XObject".to_vec())),
        ("Subtype", LObj::Name(b"Image".to_vec())),
        ("Width", LObj::Integer(8)),
        ("Height", LObj::Integer(8)),
        ("ColorSpace", LObj::Name(b"DeviceRGB".to_vec())),
        ("BitsPerComponent", LObj::Integer(8)),
    ]);
    for (k, v) in extra {
        d.set(k, v);
    }
    LStream::new(d, rgb.repeat(64))
}

/// Une page 595 × 842 : `content`, ressources `/XObject` et `/Pattern` données.
fn page_pdf(content: &str, build: impl FnOnce(&mut lopdf::Document) -> (LDict, LDict)) -> Vec<u8> {
    let mut doc = lopdf::Document::with_version("1.7");
    let (xobjects, patterns) = build(&mut doc);
    let content_id = doc.add_object(LStream::new(LDict::new(), content.as_bytes().to_vec()));
    let pages_id = doc.new_object_id();
    let mut resources = LDict::new();
    resources.set("XObject", LObj::Dictionary(xobjects));
    if !patterns.is_empty() {
        resources.set("Pattern", LObj::Dictionary(patterns));
    }
    let page = doc.add_object(LDict::from_iter(vec![
        ("Type", LObj::Name(b"Page".to_vec())),
        ("Parent", LObj::Reference(pages_id)),
        (
            "MediaBox",
            LObj::Array(vec![0.into(), 0.into(), 595.into(), 842.into()]),
        ),
        ("Resources", LObj::Dictionary(resources)),
        ("Contents", LObj::Reference(content_id)),
    ]));
    doc.objects.insert(
        pages_id,
        LObj::Dictionary(LDict::from_iter(vec![
            ("Type", LObj::Name(b"Pages".to_vec())),
            ("Kids", LObj::Array(vec![page.into()])),
            ("Count", LObj::Integer(1)),
        ])),
    );
    let catalog = doc.add_object(LDict::from_iter(vec![
        ("Type", LObj::Name(b"Catalog".to_vec())),
        ("Pages", LObj::Reference(pages_id)),
    ]));
    doc.trailer.set("Root", LObj::Reference(catalog));
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

/// Vrai si `needle` apparaît dans un flux (décompressé) du fichier.
fn in_file(bytes: &[u8], needle: &[u8]) -> bool {
    let doc = lopdf::Document::load_mem(bytes).unwrap();
    doc.objects.values().any(|o| {
        o.as_stream().is_ok_and(|s| {
            let data = s
                .decompressed_content()
                .unwrap_or_else(|_| s.content.clone());
            data.windows(needle.len()).any(|w| w == needle)
        })
    })
}

/// Un logo (image dans un formulaire XObject) loin de la zone n'empêche plus de caviarder
/// une image directe ; un formulaire qui touche la zone reste refusé.
#[test]
fn form_image_far_from_the_zone_does_not_block_redaction() {
    let bytes = page_pdf(
        "q 200 0 0 200 100 100 cm /Im Do Q q 1 0 0 1 400 700 cm /Logo Do Q",
        |doc| {
            let image = doc.add_object(rgb_image([255, 0, 0], vec![]));
            let logo_image = doc.add_object(rgb_image([0, 0, 255], vec![]));
            let logo = doc.add_object(LStream::new(
                LDict::from_iter(vec![
                    ("Type", LObj::Name(b"XObject".to_vec())),
                    ("Subtype", LObj::Name(b"Form".to_vec())),
                    (
                        "BBox",
                        LObj::Array(vec![0.into(), 0.into(), 100.into(), 50.into()]),
                    ),
                    (
                        "Resources",
                        LObj::Dictionary(LDict::from_iter(vec![(
                            "XObject",
                            LObj::Dictionary(LDict::from_iter(vec![("L", logo_image.into())])),
                        )])),
                    ),
                ]),
                b"q 100 0 0 50 0 0 cm /L Do Q".to_vec(),
            ));
            (
                LDict::from_iter(vec![("Im", image.into()), ("Logo", logo.into())]),
                LDict::new(),
            )
        },
    );
    let mut doc = Document::from_bytes(bytes.clone(), None).unwrap();
    let report = doc.redact(0, rect(100.0, 200.0, 100.0, 100.0)).unwrap();
    assert_eq!(report.images_redacted, 1);
    let mut near = Document::from_bytes(bytes, None).unwrap();
    assert!(matches!(
        near.redact(0, rect(420.0, 710.0, 30.0, 20.0)),
        Err(CoreError::InvalidOperation(_))
    ));
}

/// Les clés qui peuvent garder l'image d'origine (`/Alternates`, `/Metadata`…) ne passent
/// pas dans la copie noircie.
#[test]
fn redacted_image_drops_alternates_and_metadata() {
    let bytes = page_pdf("q 200 0 0 200 100 100 cm /Im Do Q", |doc| {
        let alt = doc.add_object(LStream::new(
            LDict::from_iter(vec![
                ("Type", LObj::Name(b"XObject".to_vec())),
                ("Subtype", LObj::Name(b"Image".to_vec())),
                ("Width", LObj::Integer(18)),
                ("Height", LObj::Integer(1)),
                ("ColorSpace", LObj::Name(b"DeviceGray".to_vec())),
                ("BitsPerComponent", LObj::Integer(8)),
            ]),
            b"ALTSECRETALTSECRET".to_vec(),
        ));
        let xmp = doc.add_object(LStream::new(
            LDict::from_iter(vec![
                ("Type", LObj::Name(b"Metadata".to_vec())),
                ("Subtype", LObj::Name(b"XML".to_vec())),
            ]),
            b"<x:xmpmeta>SECRET-THUMB</x:xmpmeta>".to_vec(),
        ));
        let alternates = LObj::Array(vec![LObj::Dictionary(LDict::from_iter(vec![
            ("Image", alt.into()),
            ("DefaultForPrinting", LObj::Boolean(true)),
        ]))]);
        let image = doc.add_object(rgb_image(
            [0, 200, 0],
            vec![("Alternates", alternates), ("Metadata", xmp.into())],
        ));
        (LDict::from_iter(vec![("Im", image.into())]), LDict::new())
    });
    let mut doc = Document::from_bytes(bytes, None).unwrap();
    assert_eq!(
        doc.redact(0, rect(100.0, 200.0, 100.0, 100.0))
            .unwrap()
            .images_redacted,
        1
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("alt.pdf");
    doc.save_as(&path).unwrap();
    let saved = std::fs::read(&path).unwrap();
    assert!(!in_file(&saved, b"ALTSECRET"), "image alternative partie");
    assert!(
        !in_file(&saved, b"SECRET-THUMB"),
        "métadonnées de l'image parties"
    );
}

/// Une image peinte par un **motif** (tiling pattern) n'est pas atteinte par le caviardage
/// par objets : refus explicite plutôt qu'un cache noir sur des pixels restés intacts.
#[test]
fn image_tiling_pattern_is_refused() {
    let bytes = page_pdf("/Pattern cs /P1 scn 100 100 200 200 re f", |doc| {
        let image = doc.add_object(rgb_image([200, 0, 200], vec![]));
        let pattern = doc.add_object(LStream::new(
            LDict::from_iter(vec![
                ("Type", LObj::Name(b"Pattern".to_vec())),
                ("PatternType", LObj::Integer(1)),
                ("PaintType", LObj::Integer(1)),
                ("TilingType", LObj::Integer(1)),
                (
                    "BBox",
                    LObj::Array(vec![0.into(), 0.into(), 20.into(), 20.into()]),
                ),
                ("XStep", LObj::Integer(20)),
                ("YStep", LObj::Integer(20)),
                (
                    "Resources",
                    LObj::Dictionary(LDict::from_iter(vec![(
                        "XObject",
                        LObj::Dictionary(LDict::from_iter(vec![("I", image.into())])),
                    )])),
                ),
            ]),
            b"q 20 0 0 20 0 0 cm /I Do Q".to_vec(),
        ));
        (LDict::new(), LDict::from_iter(vec![("P1", pattern.into())]))
    });
    let mut doc = Document::from_bytes(bytes, None).unwrap();
    match doc.redact(0, rect(150.0, 150.0, 50.0, 50.0)) {
        Err(CoreError::InvalidOperation(m)) => assert!(m.contains("motif"), "{m}"),
        other => panic!("{other:?}"),
    }
}

/// Un motif d'image (tiling pattern) avec `/PatternType 1` donné en **référence**.
fn image_pattern(doc: &mut lopdf::Document, indirect_type: bool) -> lopdf::ObjectId {
    let image = doc.add_object(rgb_image([10, 20, 30], vec![]));
    let pattern_type = if indirect_type {
        LObj::Reference(doc.add_object(LObj::Integer(1)))
    } else {
        LObj::Integer(1)
    };
    doc.add_object(LStream::new(
        LDict::from_iter(vec![
            ("Type", LObj::Name(b"Pattern".to_vec())),
            ("PatternType", pattern_type),
            ("PaintType", LObj::Integer(1)),
            ("TilingType", LObj::Integer(1)),
            (
                "BBox",
                LObj::Array(vec![0.into(), 0.into(), 20.into(), 20.into()]),
            ),
            ("XStep", LObj::Integer(20)),
            ("YStep", LObj::Integer(20)),
            (
                "Resources",
                LObj::Dictionary(LDict::from_iter(vec![(
                    "XObject",
                    LObj::Dictionary(LDict::from_iter(vec![("I", image.into())])),
                )])),
            ),
        ]),
        b"q 20 0 0 20 0 0 cm /I Do Q".to_vec(),
    ))
}

/// Revue de sécurité : un motif d'image rangé dans les ressources d'un **formulaire** qui
/// touche la zone est aussi refusé (la détection descend dans les formulaires).
#[test]
fn image_pattern_inside_a_form_is_refused() {
    let bytes = page_pdf("q 1 0 0 1 100 100 cm /Fx Do Q", |doc| {
        let pattern = image_pattern(doc, false);
        let form = doc.add_object(LStream::new(
            LDict::from_iter(vec![
                ("Type", LObj::Name(b"XObject".to_vec())),
                ("Subtype", LObj::Name(b"Form".to_vec())),
                (
                    "BBox",
                    LObj::Array(vec![0.into(), 0.into(), 200.into(), 200.into()]),
                ),
                (
                    "Resources",
                    LObj::Dictionary(LDict::from_iter(vec![(
                        "Pattern",
                        LObj::Dictionary(LDict::from_iter(vec![("P1", pattern.into())])),
                    )])),
                ),
            ]),
            b"/Pattern cs /P1 scn 0 0 200 200 re f".to_vec(),
        ));
        (LDict::from_iter(vec![("Fx", form.into())]), LDict::new())
    });
    let mut doc = Document::from_bytes(bytes, None).unwrap();
    assert!(matches!(
        doc.redact(0, rect(150.0, 150.0, 50.0, 50.0)),
        Err(CoreError::InvalidOperation(_))
    ));
}

/// Revue de sécurité : `/PatternType` en objet indirect ne fait pas passer le motif.
#[test]
fn indirect_pattern_type_is_still_detected() {
    let bytes = page_pdf("/Pattern cs /P1 scn 100 100 200 200 re f", |doc| {
        let pattern = image_pattern(doc, true);
        (LDict::new(), LDict::from_iter(vec![("P1", pattern.into())]))
    });
    let mut doc = Document::from_bytes(bytes, None).unwrap();
    assert!(matches!(
        doc.redact(0, rect(150.0, 150.0, 50.0, 50.0)),
        Err(CoreError::InvalidOperation(_))
    ));
}

/// Revue de sécurité : `/PieceInfo` (données privées d'un éditeur) ne passe pas non plus
/// dans la copie noircie.
#[test]
fn redacted_image_drops_piece_info() {
    let bytes = page_pdf("q 200 0 0 200 100 100 cm /Im Do Q", |doc| {
        let private = doc.add_object(LStream::new(LDict::new(), b"SECRET-PIECE".to_vec()));
        let piece = LObj::Dictionary(LDict::from_iter(vec![(
            "Editeur",
            LObj::Dictionary(LDict::from_iter(vec![("Private", private.into())])),
        )]));
        let image = doc.add_object(rgb_image([0, 200, 0], vec![("PieceInfo", piece)]));
        (LDict::from_iter(vec![("Im", image.into())]), LDict::new())
    });
    let mut doc = Document::from_bytes(bytes, None).unwrap();
    doc.redact(0, rect(100.0, 200.0, 100.0, 100.0)).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("piece.pdf");
    doc.save_as(&path).unwrap();
    assert!(!in_file(&std::fs::read(&path).unwrap(), b"SECRET-PIECE"));
}

/// Revue de sécurité : une `/Matrix` de formulaire illisible ne vaut pas l'identité (qui
/// pourrait placer le formulaire loin de la zone) : le formulaire compte comme touchant la
/// zone, et son image fait refuser.
#[test]
fn unreadable_form_matrix_fails_closed() {
    let bytes = page_pdf("q 1 0 0 1 0 0 cm /Fx Do Q", |doc| {
        let image = doc.add_object(rgb_image([1, 2, 3], vec![]));
        let form = doc.add_object(LStream::new(
            LDict::from_iter(vec![
                ("Type", LObj::Name(b"XObject".to_vec())),
                ("Subtype", LObj::Name(b"Form".to_vec())),
                (
                    "BBox",
                    LObj::Array(vec![0.into(), 0.into(), 10.into(), 10.into()]),
                ),
                // Matrice qui emmène le formulaire sur la zone, mais mal formée (5 nombres).
                (
                    "Matrix",
                    LObj::Array(vec![20.into(), 0.into(), 0.into(), 20.into(), 100.into()]),
                ),
                (
                    "Resources",
                    LObj::Dictionary(LDict::from_iter(vec![(
                        "XObject",
                        LObj::Dictionary(LDict::from_iter(vec![("I", image.into())])),
                    )])),
                ),
            ]),
            b"q 10 0 0 10 0 0 cm /I Do Q".to_vec(),
        ));
        (LDict::from_iter(vec![("Fx", form.into())]), LDict::new())
    });
    let mut doc = Document::from_bytes(bytes, None).unwrap();
    assert!(matches!(
        doc.redact(0, rect(150.0, 150.0, 40.0, 40.0)),
        Err(CoreError::InvalidOperation(_))
    ));
}

/// Revue de sécurité : un formulaire **sans** `/Resources` hérite de celles de la page ; une
/// image qu'il dessinerait par elles ne doit pas rester dans le fichier : soit le caviardage
/// est refusé, soit ses pixels ont disparu du fichier enregistré.
#[test]
fn form_without_resources_inherits_the_page_ones() {
    let bytes = page_pdf("q 1 0 0 1 100 100 cm /Fx Do Q", |doc| {
        let image = doc.add_object(rgb_image([4, 5, 6], vec![]));
        let form = doc.add_object(LStream::new(
            LDict::from_iter(vec![
                ("Type", LObj::Name(b"XObject".to_vec())),
                ("Subtype", LObj::Name(b"Form".to_vec())),
                (
                    "BBox",
                    LObj::Array(vec![0.into(), 0.into(), 200.into(), 200.into()]),
                ),
            ]),
            b"q 200 0 0 200 0 0 cm /Im Do Q".to_vec(),
        ));
        (
            LDict::from_iter(vec![("Fx", form.into()), ("Im", image.into())]),
            LDict::new(),
        )
    });
    let mut doc = Document::from_bytes(bytes, None).unwrap();
    match doc.redact(0, rect(150.0, 150.0, 40.0, 40.0)) {
        Err(CoreError::InvalidOperation(_)) => {}
        Ok(_) => {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("herite.pdf");
            doc.save_as(&path).unwrap();
            let pixels = [4u8, 5, 6].repeat(8);
            assert!(
                !in_file(&std::fs::read(&path).unwrap(), &pixels),
                "image restée"
            );
        }
        Err(other) => panic!("{other:?}"),
    }
}

/// Revue de sécurité : une image dont le `/Subtype` est une référence indirecte est bien
/// reconnue et noircie (elle n'échappe pas au caviardage).
#[test]
fn image_with_indirect_subtype_is_redacted() {
    let bytes = page_pdf("q 200 0 0 200 100 100 cm /Im Do Q", |doc| {
        let subtype = doc.add_object(LObj::Name(b"Image".to_vec()));
        let image = doc.add_object(rgb_image([7, 8, 9], vec![("Subtype", subtype.into())]));
        (LDict::from_iter(vec![("Im", image.into())]), LDict::new())
    });
    let mut doc = Document::from_bytes(bytes, None).unwrap();
    let report = doc.redact(0, rect(100.0, 200.0, 100.0, 100.0)).unwrap();
    assert_eq!(
        report.images_redacted + report.images_removed,
        1,
        "{report:?}"
    );
}
