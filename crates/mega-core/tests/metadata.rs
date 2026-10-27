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
fn set_metadata_validates_and_refuses_encrypted() {
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
    match protected.set_metadata(&MetadataPatch {
        title: Some("x".to_string()),
        ..MetadataPatch::default()
    }) {
        Err(CoreError::InvalidOperation(m)) => assert!(m.contains("chiffré"), "{m}"),
        other => panic!("{other:?}"),
    }
}
