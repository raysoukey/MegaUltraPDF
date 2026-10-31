// SPDX-License-Identifier: GPL-3.0-or-later
//! Métadonnées modifiables (`/Info`), écrites par `lopdf`.

use mega_core::edit::EditCommand;
use mega_core::properties::MetadataPatch;
use mega_core::testing::sample_pdf;
use mega_core::{CoreError, Document};

fn doc() -> Document {
    Document::from_bytes(sample_pdf(1, "Avant"), None).unwrap()
}

#[test]
fn set_metadata_round_trips_unicode() {
    let mut d = doc();
    d.set_metadata(&MetadataPatch {
        title: Some("Résumé — été".to_string()),
        author: Some("Zoë".to_string()),
        subject: Some("Sujet".to_string()),
        keywords: Some("un, deux".to_string()),
    })
    .unwrap();
    let p = d.properties().unwrap();
    assert_eq!(p.title.as_deref(), Some("Résumé — été"));
    assert_eq!(p.author.as_deref(), Some("Zoë"));
    assert_eq!(p.subject.as_deref(), Some("Sujet"));
    assert_eq!(p.keywords.as_deref(), Some("un, deux"));
    assert_eq!(d.title().as_deref(), Some("Résumé — été"));
    let reopened = Document::from_bytes(d.to_bytes().unwrap(), None).unwrap();
    let p = reopened.properties().unwrap();
    assert_eq!(p.title.as_deref(), Some("Résumé — été"));
    assert_eq!(p.author.as_deref(), Some("Zoë"));
    // Les autres clés (producteur du squelette de test) sont conservées.
    assert_eq!(p.producer.as_deref(), Some("MegaUltraPDF tests"));
}

#[test]
fn set_metadata_empty_removes_key_and_none_keeps_it() {
    let mut d = doc();
    d.set_metadata(&MetadataPatch {
        author: Some("Quelqu'un".to_string()),
        ..MetadataPatch::default()
    })
    .unwrap();
    assert_eq!(
        d.properties().unwrap().title.as_deref(),
        Some("Avant"),
        "None = inchangé"
    );
    d.set_metadata(&MetadataPatch {
        title: Some(String::new()),
        ..MetadataPatch::default()
    })
    .unwrap();
    let p = d.properties().unwrap();
    assert_eq!(p.title, None, "chaîne vide = clé retirée");
    assert_eq!(p.author.as_deref(), Some("Quelqu'un"));
    let raw = String::from_utf8_lossy(&d.to_bytes().unwrap()).into_owned();
    assert!(!raw.contains("/Title"), "{raw}");
}

#[test]
fn set_metadata_is_undoable_and_updates_mod_date() {
    let mut d = doc();
    let before = d.properties().unwrap().modification_date;
    d.apply(&EditCommand::SetMetadata {
        patch: MetadataPatch {
            title: Some("Après".to_string()),
            ..MetadataPatch::default()
        },
    })
    .unwrap();
    assert_eq!(d.undo_label().as_deref(), Some("Modifier les métadonnées"));
    assert!(d.is_modified());
    let after = d.properties().unwrap().modification_date;
    assert!(after.is_some() && after != before, "{after:?}");
    assert!(after.as_deref().unwrap().starts_with("D:2"));
    d.undo().unwrap();
    assert_eq!(d.properties().unwrap().title.as_deref(), Some("Avant"));
}

#[test]
fn set_metadata_validates_and_keeps_owner_protection() {
    let mut d = doc();
    let long = "x".repeat(1025);
    assert!(matches!(
        d.set_metadata(&MetadataPatch {
            title: Some(long),
            ..MetadataPatch::default()
        }),
        Err(CoreError::InvalidOperation(_))
    ));
    let bytes = std::fs::read(format!(
        "{}/../../fixtures/owner-only.pdf",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let mut protected = Document::from_bytes(bytes, None).unwrap();
    // Lot 10 : la protection « propriétaire seul » est conservée (rechiffrement).
    protected
        .set_metadata(&MetadataPatch {
            title: Some("x".to_string()),
            ..MetadataPatch::default()
        })
        .unwrap();
    assert_eq!(protected.title().as_deref(), Some("x"));
    let out = protected.to_bytes().unwrap();
    assert!(String::from_utf8_lossy(&out).contains("/Encrypt"));
}

// --- Lot 9 : XMP, /Info direct, contrôles, /ModDate (C11-C16) ----------------------------

fn title(text: &str) -> MetadataPatch {
    MetadataPatch {
        title: Some(text.to_string()),
        ..Default::default()
    }
}

/// Le texte du flux `/Metadata` du catalogue.
fn xmp_of(bytes: &[u8]) -> String {
    let doc = lopdf::Document::load_mem(bytes).unwrap();
    let id = doc
        .catalog()
        .unwrap()
        .get(b"Metadata")
        .unwrap()
        .as_reference()
        .unwrap();
    let stream = doc.get_object(id).unwrap().as_stream().unwrap();
    let data = stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone());
    String::from_utf8(data).unwrap()
}

#[test]
fn set_metadata_updates_xmp_title() {
    let mut d = Document::from_bytes(mega_core::testing::xmp_pdf(), None).unwrap();
    d.set_metadata(&MetadataPatch {
        title: Some("Nouveau <titre> & co".to_string()),
        author: Some("Bob".to_string()),
        ..Default::default()
    })
    .unwrap();
    let xmp = xmp_of(&d.to_bytes().unwrap());
    assert!(xmp.contains("Nouveau &lt;titre&gt; &amp; co"), "{xmp}");
    assert!(!xmp.contains("Ancien"), "{xmp}");
    assert!(xmp.contains("<rdf:li>Bob</rdf:li>"), "{xmp}");
    assert!(
        !xmp.contains("2020-01-01"),
        "ModifyDate mise à jour : {xmp}"
    );
}

#[test]
fn direct_info_keeps_its_keys() {
    let mut d = Document::from_bytes(mega_core::testing::direct_info_pdf(), None).unwrap();
    d.set_metadata(&title("Nouveau")).unwrap();
    let p = d.properties().unwrap();
    assert_eq!(p.title.as_deref(), Some("Nouveau"));
    assert_eq!(
        p.author.as_deref(),
        Some("Alice"),
        "clé du /Info direct conservée"
    );
}

#[test]
fn control_characters_in_metadata_are_refused() {
    let mut d = doc();
    assert!(matches!(
        d.set_metadata(&title("a\u{7}b")),
        Err(CoreError::InvalidOperation(_))
    ));
    // La tabulation reste permise.
    d.set_metadata(&title("a\tb")).unwrap();
}

#[test]
fn save_sets_mod_date() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("date.pdf");
    let mut d = doc();
    assert_eq!(d.properties().unwrap().modification_date, None);
    d.apply(&EditCommand::RotatePages {
        pages: vec![0],
        quarter_turns: 1,
    })
    .unwrap();
    d.save_as(&path).unwrap();
    let reopened = Document::open(&path, None).unwrap();
    let date = reopened.properties().unwrap().modification_date;
    assert!(
        date.as_deref().is_some_and(|d| d.starts_with("D:20")),
        "{date:?}"
    );
}

#[test]
fn mod_date_of_a_user_password_file_is_read() {
    use mega_core::security::{Permissions, Protection};
    let mut d = doc();
    d.set_metadata(&title("Daté")).unwrap();
    d.protect(&Protection {
        user_password: "secret".to_string(),
        owner_password: "patron".to_string(),
        permissions: Permissions {
            print: true,
            copy: true,
            modify: true,
            annotate: true,
        },
    })
    .unwrap();
    let reopened = Document::from_bytes(d.to_bytes().unwrap(), Some("secret")).unwrap();
    let date = reopened.properties().unwrap().modification_date;
    assert!(
        date.as_deref().is_some_and(|d| d.starts_with("D:20")),
        "{date:?}"
    );
}

/// C6 (audit du Lot 13) : une déclaration PDF/A est lue, et conservée (identification XMP,
/// `/OutputIntents`) après une modification et après une optimisation, pour que l'application
/// puisse prévenir que la conformité n'est plus garantie.
#[test]
fn a_pdfa_claim_is_detected_and_kept() {
    let mut doc = Document::from_bytes(mega_core::testing::pdfa_pdf(), None).unwrap();
    assert_eq!(doc.properties().unwrap().pdfa.as_deref(), Some("PDF/A-2B"));
    assert_eq!(
        Document::from_bytes(mega_core::testing::sample_pdf(1, "x"), None)
            .unwrap()
            .properties()
            .unwrap()
            .pdfa,
        None
    );
    doc.apply(&mega_core::edit::EditCommand::RotatePages {
        pages: vec![0],
        quarter_turns: 1,
    })
    .unwrap();
    doc.optimize(&mega_core::OptimizeOptions::default())
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("archive.pdf");
    doc.save_as(&out).unwrap();
    let bytes = std::fs::read(&out).unwrap();
    let reopened = Document::from_bytes(bytes.clone(), None).unwrap();
    assert_eq!(
        reopened.properties().unwrap().pdfa.as_deref(),
        Some("PDF/A-2B")
    );
    let lo = lopdf::Document::load_mem(&bytes).unwrap();
    let catalog = lo.catalog().unwrap();
    assert!(catalog.has(b"OutputIntents"), "profil de sortie perdu");
    assert!(catalog.has(b"Metadata"), "métadonnées XMP perdues");
}

#[test]
fn pdfa_4_documents_do_not_receive_an_info_dictionary() {
    let mut lo = lopdf::Document::load_mem(&mega_core::testing::pdfa_pdf()).unwrap();
    let meta = lo
        .catalog()
        .unwrap()
        .get(b"Metadata")
        .unwrap()
        .as_reference()
        .unwrap();
    let stream = lo.get_object_mut(meta).unwrap().as_stream_mut().unwrap();
    let xmp = String::from_utf8(stream.content.clone()).unwrap().replace(
        "pdfaid:part=\"2\" pdfaid:conformance=\"B\"/>",
        "pdfaid:part=\"4\" xmp:ModifyDate=\"2001-01-01T00:00:00Z\"          xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\"/>",
    );
    stream.set_plain_content(xmp.into_bytes());
    lo.trailer.remove(b"Info");
    let mut bytes = Vec::new();
    lo.save_to(&mut bytes).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pdfa4.pdf");
    let mut d = Document::from_bytes(bytes, None).unwrap();
    d.rotate_pages(&[0], 1).unwrap();
    d.save_as(&path).unwrap();
    let out = lopdf::Document::load_mem(&std::fs::read(&path).unwrap()).unwrap();
    assert!(out.trailer.get(b"Info").is_err(), "PDF/A-4 : pas de /Info");
    let meta = out
        .catalog()
        .unwrap()
        .get(b"Metadata")
        .unwrap()
        .as_reference()
        .unwrap();
    let xmp = String::from_utf8(
        out.get_object(meta)
            .unwrap()
            .as_stream()
            .unwrap()
            .decompressed_content()
            .unwrap(),
    )
    .unwrap();
    assert!(
        !xmp.contains("2001-01-01"),
        "xmp:ModifyDate mis à jour : {xmp}"
    );
}

/// `pdfa_pdf` déclaré en partie `part`, avec ou sans `/Info` réduit à un `/ModDate` ancien.
fn pdfa_with(part: &str, info: Option<&str>) -> Vec<u8> {
    let mut lo = lopdf::Document::load_mem(&mega_core::testing::pdfa_pdf()).unwrap();
    let meta = lo
        .catalog()
        .unwrap()
        .get(b"Metadata")
        .unwrap()
        .as_reference()
        .unwrap();
    let stream = lo.get_object_mut(meta).unwrap().as_stream_mut().unwrap();
    let xmp = String::from_utf8(stream.content.clone())
        .unwrap()
        .replace("pdfaid:part=\"2\"", &format!("pdfaid:part=\"{part}\""));
    stream.set_plain_content(xmp.into_bytes());
    lo.trailer.remove(b"Info");
    if let Some(date) = info {
        let id = lo.add_object(lopdf::Dictionary::from_iter(vec![(
            "ModDate",
            lopdf::Object::string_literal(date),
        )]));
        lo.trailer.set("Info", id);
    }
    let mut bytes = Vec::new();
    lo.save_to(&mut bytes).unwrap();
    bytes
}

fn saved_info_mod_date(bytes: Vec<u8>) -> Option<String> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sortie.pdf");
    let mut d = Document::from_bytes(bytes, None).unwrap();
    d.rotate_pages(&[0], 1).unwrap();
    d.save_as(&path).unwrap();
    let out = lopdf::Document::load_mem(&std::fs::read(&path).unwrap()).unwrap();
    let info = out.trailer.get(b"Info").ok()?;
    let (_, info) = out.dereference(info).ok()?;
    let date = info.as_dict().ok()?.get(b"ModDate").ok()?.as_str().ok()?;
    Some(String::from_utf8_lossy(date).into_owned())
}

#[test]
fn pdfa_4_existing_info_keeps_getting_its_mod_date() {
    let date = saved_info_mod_date(pdfa_with("4", Some("D:20010101000000+00'00'")));
    assert!(
        date.is_some_and(|d| !d.contains("2001")),
        "ModDate mis à jour"
    );
}

#[test]
fn pdfa_2_documents_still_receive_a_mod_date() {
    let date = saved_info_mod_date(pdfa_with("2", None));
    assert!(
        date.is_some_and(|d| d.starts_with("D:")),
        "/Info créé comme avant"
    );
}

/// PDF sans `/Info` dont le XMP porte les cinq propriétés repliées (audit L12).
fn xmp_only_full() -> Document {
    let xmp = "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\"><dc:title><rdf:Alt><rdf:li xml:lang=\"fr\">Autre</rdf:li><rdf:li xml:lang=\"x-default\">Ancien</rdf:li></rdf:Alt></dc:title><dc:creator><rdf:Seq><rdf:li>Alice</rdf:li><rdf:li>Bob</rdf:li></rdf:Seq></dc:creator><dc:description><rdf:Alt><rdf:li xml:lang=\"x-default\">Sujet XMP</rdf:li></rdf:Alt></dc:description><pdf:Keywords>un, deux</pdf:Keywords><xmp:CreateDate>2020-01-02T03:04:05Z</xmp:CreateDate></rdf:Description></rdf:RDF></x:xmpmeta>";
    let mut lo =
        lopdf::Document::load_mem(&mega_core::testing::metadata_variant_pdf("xmp-only")).unwrap();
    let id = lo
        .catalog()
        .unwrap()
        .get(b"Metadata")
        .unwrap()
        .as_reference()
        .unwrap();
    let stream = lo.get_object_mut(id).unwrap().as_stream_mut().unwrap();
    stream.set_plain_content(xmp.as_bytes().to_vec());
    let mut out = Vec::new();
    lo.save_to(&mut out).unwrap();
    Document::from_bytes(out, None).unwrap()
}

#[test]
fn title_falls_back_to_xmp_dc_title_when_info_is_absent() {
    let d =
        Document::from_bytes(mega_core::testing::metadata_variant_pdf("xmp-only"), None).unwrap();
    let p = d.properties().unwrap();
    assert_eq!(p.title.as_deref(), Some("Ancien"));
    assert_eq!(p.author.as_deref(), Some("Alice"));
}

#[test]
fn xmp_fallback_covers_author_subject_keywords_and_creation_date() {
    let p = xmp_only_full().properties().unwrap();
    assert_eq!(p.title.as_deref(), Some("Ancien"), "x-default préféré");
    assert_eq!(p.author.as_deref(), Some("Alice"));
    assert_eq!(p.subject.as_deref(), Some("Sujet XMP"));
    assert_eq!(p.keywords.as_deref(), Some("un, deux"));
    assert_eq!(p.creation_date.as_deref(), Some("D:20200102030405Z"));
}

#[test]
fn info_wins_over_the_xmp_fallback() {
    let d = Document::from_bytes(mega_core::testing::xmp_pdf(), None).unwrap();
    let mut d = d;
    d.set_metadata(&MetadataPatch {
        title: Some("Info".to_string()),
        ..MetadataPatch::default()
    })
    .unwrap();
    assert_eq!(d.properties().unwrap().title.as_deref(), Some("Info"));
}

#[test]
fn user_unit_is_reported() {
    let d = Document::from_bytes(mega_core::testing::user_unit_pdf(), None).unwrap();
    assert_eq!(d.properties().unwrap().user_unit, Some(10.0));
    let plain = Document::from_bytes(sample_pdf(1, "x"), None).unwrap();
    assert_eq!(plain.properties().unwrap().user_unit, None);
    let simple = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/simple.pdf"
    ))
    .unwrap();
    let simple = Document::from_bytes(simple, None).unwrap();
    assert_eq!(simple.properties().unwrap().user_unit, None);
}

/// Document sans `/Info` dont le XMP ne contient que `description` (le contenu de
/// `rdf:Description`).
fn doc_with_xmp(description: &str) -> Document {
    let xmp = format!(
        "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF \
         xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
         <rdf:Description rdf:about=\"\">{description}</rdf:Description></rdf:RDF></x:xmpmeta>"
    );
    let mut lo =
        lopdf::Document::load_mem(&mega_core::testing::metadata_variant_pdf("xmp-only")).unwrap();
    let id = lo
        .catalog()
        .unwrap()
        .get(b"Metadata")
        .unwrap()
        .as_reference()
        .unwrap();
    let stream = lo.get_object_mut(id).unwrap().as_stream_mut().unwrap();
    stream.set_plain_content(xmp.into_bytes());
    let mut out = Vec::new();
    lo.save_to(&mut out).unwrap();
    Document::from_bytes(out, None).unwrap()
}

fn xmp_title(raw: &str) -> Option<String> {
    let li = format!(
        "<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{raw}</rdf:li></rdf:Alt></dc:title>"
    );
    doc_with_xmp(&li).properties().unwrap().title
}

#[test]
fn xmp_entities_cdata_and_control_characters_are_cleaned() {
    assert_eq!(xmp_title("A &amp; B").as_deref(), Some("A & B"));
    assert_eq!(xmp_title("caf&#233;").as_deref(), Some("café"));
    assert_eq!(xmp_title("caf&#xE9;").as_deref(), Some("café"));
    assert_eq!(
        xmp_title("<![CDATA[a <b> &amp; c]]>").as_deref(),
        Some("a <b> &amp; c")
    );
    assert_eq!(xmp_title("a&#0;b&#xD800;c").as_deref(), Some("abc"));
    assert_eq!(
        xmp_title("un\ndeux\u{0}trois\u{7f}").as_deref(),
        Some("un deuxtrois")
    );
}

#[test]
fn an_oversize_xmp_title_is_cut_to_the_metadata_limit() {
    let long = "x".repeat(2000);
    let title = xmp_title(&long).unwrap();
    assert_eq!(
        title.chars().count(),
        mega_core::properties::MAX_METADATA_CHARS
    );
}

#[test]
fn xmp_create_date_keeps_its_time_zone() {
    let d = doc_with_xmp("<xmp:CreateDate>2020-01-02T03:04:05+01:00</xmp:CreateDate>");
    assert_eq!(
        d.properties().unwrap().creation_date.as_deref(),
        Some("D:20200102030405+01'00'")
    );
}

fn user_unit_doc(value: lopdf::Object) -> Document {
    let mut lo = lopdf::Document::load_mem(&mega_core::testing::user_unit_pdf()).unwrap();
    let page = *lo.get_pages().values().next().unwrap();
    lo.get_dictionary_mut(page).unwrap().set("UserUnit", value);
    let mut out = Vec::new();
    lo.save_to(&mut out).unwrap();
    Document::from_bytes(out, None).unwrap()
}

#[test]
fn user_unit_accepts_reals_and_is_bounded() {
    let unit = |v| user_unit_doc(v).properties().unwrap().user_unit;
    assert_eq!(unit(lopdf::Object::Real(0.5)), Some(0.5));
    assert_eq!(unit(lopdf::Object::Real(75_000.0)), Some(75_000.0));
    assert_eq!(unit(lopdf::Object::Real(75_001.0)), None);
    assert_eq!(unit(lopdf::Object::Real(0.0)), None);
    assert_eq!(unit(lopdf::Object::Real(-2.0)), None);
    assert_eq!(unit(lopdf::Object::Integer(1)), None);
}
