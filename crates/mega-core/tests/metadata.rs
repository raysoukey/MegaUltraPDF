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
