// SPDX-License-Identifier: GPL-3.0-or-later
//! Pièces jointes : liste, extraction, ajout, suppression.

use mega_core::edit::EditCommand;
use mega_core::security::{Permissions, Protection};
use mega_core::testing::{attachments_pdf, sample_pdf, third_party_annots_pdf};
use mega_core::{CoreError, Document};

fn with_attachments() -> Document {
    Document::from_bytes(attachments_pdf(), None).unwrap()
}

#[test]
fn lists_embedded_files() {
    let doc = with_attachments();
    let list = doc.attachments().unwrap();
    assert_eq!(list.len(), 2);
    // PDFium suit l'ordre de l'arbre de noms (clés triées) : « donnees.csv » avant « notes.txt ».
    assert_eq!(list[0].index, 0);
    assert_eq!(list[0].name, "données.csv");
    assert_eq!(list[0].size, 19);
    assert_eq!(list[1].name, "notes.txt");
    assert_eq!(list[1].size, 13);
    let plain = Document::from_bytes(sample_pdf(1, "x"), None).unwrap();
    assert!(plain.attachments().unwrap().is_empty());
}

#[test]
fn save_attachment_writes_exact_bytes() {
    let doc = with_attachments();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("notes.txt");
    doc.save_attachment(1, &out).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), b"Bonjour monde");
    let csv = dir.path().join("d.csv");
    doc.save_attachment(0, &csv).unwrap();
    assert_eq!(std::fs::read(&csv).unwrap(), b"a,b\n1,2\n3,4\n5,6\n7,8");
    assert!(matches!(
        doc.save_attachment(2, &dir.path().join("x")),
        Err(CoreError::InvalidOperation(_))
    ));
}

#[test]
fn add_and_delete_attachment_are_undoable() {
    let mut doc = with_attachments();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("rapport.md");
    std::fs::write(&file, b"# Rapport\n").unwrap();
    doc.apply(&EditCommand::AddAttachment {
        path: file.to_string_lossy().into_owned(),
    })
    .unwrap();
    assert_eq!(doc.undo_label().as_deref(), Some("Joindre : rapport.md"));
    let list = doc.attachments().unwrap();
    assert_eq!(list.len(), 3);
    assert_eq!(list[2].name, "rapport.md");
    assert_eq!(list[2].size, 10);
    // Survit à la sérialisation.
    let reopened = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    assert_eq!(reopened.attachments().unwrap().len(), 3);
    let out = dir.path().join("relu.md");
    reopened.save_attachment(2, &out).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), b"# Rapport\n");
    // Même nom une seconde fois : dédoublonné (la liste reste triée par nom).
    doc.apply(&EditCommand::AddAttachment {
        path: file.to_string_lossy().into_owned(),
    })
    .unwrap();
    let names: Vec<String> = doc
        .attachments()
        .unwrap()
        .into_iter()
        .map(|a| a.name)
        .collect();
    assert!(names.contains(&"rapport (2).md".to_string()), "{names:?}");
    doc.undo().unwrap();
    doc.apply(&EditCommand::DeleteAttachment { index: 1 })
        .unwrap();
    assert_eq!(
        doc.undo_label().as_deref(),
        Some("Retirer la pièce jointe : notes.txt")
    );
    let names: Vec<String> = doc
        .attachments()
        .unwrap()
        .into_iter()
        .map(|a| a.name)
        .collect();
    assert_eq!(names, vec!["données.csv", "rapport.md"]);
    doc.undo().unwrap();
    assert_eq!(doc.attachments().unwrap().len(), 3);
    doc.undo().unwrap();
    assert_eq!(doc.attachments().unwrap().len(), 2);
}

#[test]
fn attachment_too_big_or_missing_is_refused() {
    let mut doc = with_attachments();
    let dir = tempfile::tempdir().unwrap();
    let huge = dir.path().join("creux.bin");
    let f = std::fs::File::create(&huge).unwrap();
    f.set_len(101 * 1024 * 1024).unwrap();
    drop(f);
    match doc.add_attachment(&huge) {
        Err(CoreError::InvalidOperation(m)) => assert!(m.contains("100"), "{m}"),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        doc.add_attachment(&dir.path().join("absent.txt")),
        Err(CoreError::Io(_))
    ));
    assert!(matches!(
        doc.delete_attachment(5),
        Err(CoreError::InvalidOperation(_))
    ));
    assert_eq!(doc.attachments().unwrap().len(), 2, "rien n'a changé");
}

/// Audit L11 : une annotation `FileAttachment` est une pièce jointe de page.
#[test]
fn file_attachment_annotations_are_listed_with_their_page_and_extracted() {
    let doc = Document::from_bytes(third_party_annots_pdf(), None).unwrap();
    let list = doc.attachments().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].index, 0);
    assert_eq!(list[0].name, "piece.txt");
    assert_eq!(list[0].size, 7);
    assert_eq!(list[0].page, Some(0));
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("piece.txt");
    doc.save_attachment(0, &out).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), b"contenu");
}

#[test]
fn file_attachment_in_a_password_protected_document_is_extracted() {
    let mut doc = Document::from_bytes(third_party_annots_pdf(), None).unwrap();
    doc.protect(&Protection {
        user_password: "ouvre".to_string(),
        owner_password: "maitre".to_string(),
        permissions: Permissions {
            print: true,
            copy: true,
            modify: true,
            annotate: true,
        },
    })
    .unwrap();
    let protected = doc.to_bytes().unwrap();
    let reopened = Document::from_bytes(protected, Some("ouvre")).unwrap();
    let list = reopened.attachments().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].page, Some(0));
    assert_eq!(list[0].size, 7);
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("p.txt");
    reopened.save_attachment(0, &out).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), b"contenu");
}

/// Supprimer une pièce jointe de page supprime son annotation.
#[test]
fn deleting_a_page_attachment_removes_its_annotation() {
    let mut doc = Document::from_bytes(third_party_annots_pdf(), None).unwrap();
    doc.delete_attachment(0).unwrap();
    assert!(doc.attachments().unwrap().is_empty());
    assert_eq!(doc.annotations(0).unwrap().len(), 5);
}
