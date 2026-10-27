// SPDX-License-Identifier: GPL-3.0-or-later
//! Objets de page : lecture, suppression, déplacement, remplacement du texte.

use mega_core::edit::EditCommand;
use mega_core::testing::{rotated_pdf, sample_pdf};
use mega_core::{CoreError, Document, ObjectKind, Rect, SignatureSource};

fn one_page() -> Document {
    Document::from_bytes(sample_pdf(1, "objets"), None).unwrap()
}

fn page_text(doc: &Document, page: u16) -> String {
    doc.text_boxes(page)
        .unwrap()
        .iter()
        .map(|b| b.ch)
        .filter(|c| !c.is_whitespace())
        .collect()
}

fn close(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn reads_page_objects() {
    let doc = one_page();
    let objects = doc.page_objects(0).unwrap();
    assert_eq!(objects.len(), 1);
    let o = &objects[0];
    assert_eq!(o.index, 0);
    assert_eq!(o.kind, ObjectKind::Text);
    assert_eq!(o.text.as_deref(), Some("Page 1"));
    assert!(close(o.font_size.unwrap(), 24.0, 0.5), "{:?}", o.font_size);
    // « Page 1 » en Helvetica 24 posé à (72, 760) : emprise plausible.
    assert!(close(o.bounds.x, 72.0, 3.0), "{:?}", o.bounds);
    assert!(o.bounds.y > 740.0 && o.bounds.y < 762.0, "{:?}", o.bounds);
    assert!(o.bounds.w > 50.0 && o.bounds.w < 120.0, "{:?}", o.bounds);
    assert!(o.bounds.h > 10.0 && o.bounds.h < 30.0, "{:?}", o.bounds);
}

#[test]
fn delete_object_changes_rendering_and_undo() {
    let mut doc = one_page();
    let before = doc.render_page(0, 0.5).unwrap();
    doc.apply(&EditCommand::DeleteObject { page: 0, index: 0 })
        .unwrap();
    assert_eq!(doc.undo_label().as_deref(), Some("Supprimer : Texte"));
    assert!(doc.page_objects(0).unwrap().is_empty());
    assert_eq!(page_text(&doc, 0), "");
    let after = doc.render_page(0, 0.5).unwrap();
    assert_ne!(before.rgba, after.rgba);
    // La suppression survit à la sérialisation.
    let reopened = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    assert!(reopened.page_objects(0).unwrap().is_empty());
    doc.undo().unwrap();
    assert_eq!(doc.page_objects(0).unwrap().len(), 1);
    assert_eq!(doc.render_page(0, 0.5).unwrap().rgba, before.rgba);
}

#[test]
fn move_object_shifts_bounds() {
    let mut doc = one_page();
    let before = doc.page_objects(0).unwrap()[0].bounds;
    doc.apply(&EditCommand::MoveObject {
        page: 0,
        index: 0,
        dx: 10.0,
        dy: -20.0,
    })
    .unwrap();
    assert_eq!(doc.undo_label().as_deref(), Some("Déplacer : Texte"));
    let after = doc.page_objects(0).unwrap()[0].bounds;
    assert!(close(after.x, before.x + 10.0, 0.1), "{after:?}");
    assert!(close(after.y, before.y - 20.0, 0.1), "{after:?}");
    assert!(close(after.w, before.w, 0.1) && close(after.h, before.h, 0.1));
    let reopened = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    let kept = reopened.page_objects(0).unwrap()[0].bounds;
    assert!(close(kept.x, after.x, 0.1) && close(kept.y, after.y, 0.1));
}

#[test]
fn move_object_on_rotated_page_follows_displayed_axes() {
    let mut doc = Document::from_bytes(rotated_pdf(), None).unwrap();
    let before = doc.page_objects(0).unwrap()[0].bounds;
    doc.move_object(0, 0, 10.0, 0.0).unwrap();
    let after = doc.page_objects(0).unwrap()[0].bounds;
    assert!(
        close(after.x, before.x + 10.0, 0.1),
        "{before:?} → {after:?}"
    );
    assert!(close(after.y, before.y, 0.1), "{before:?} → {after:?}");
}

#[test]
fn set_object_text_round_trips() {
    let mut doc = one_page();
    doc.apply(&EditCommand::SetObjectText {
        page: 0,
        index: 0,
        text: "Bonjour".to_string(),
    })
    .unwrap();
    assert_eq!(doc.undo_label().as_deref(), Some("Modifier le texte"));
    assert_eq!(page_text(&doc, 0), "Bonjour");
    assert_eq!(
        doc.page_objects(0).unwrap()[0].text.as_deref(),
        Some("Bonjour")
    );
    let reopened = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    assert_eq!(page_text(&reopened, 0), "Bonjour");
    doc.undo().unwrap();
    assert_eq!(page_text(&doc, 0), "Page1");
}

#[test]
fn unknown_object_index_is_refused() {
    let mut doc = one_page();
    for result in [
        doc.delete_object(0, 5),
        doc.move_object(0, 5, 1.0, 1.0),
        doc.set_object_text(0, 5, "x"),
    ] {
        assert!(
            matches!(result, Err(CoreError::InvalidOperation(_))),
            "{result:?}"
        );
    }
    assert!(matches!(
        doc.page_objects(3),
        Err(CoreError::PageOutOfRange { .. })
    ));
    assert_eq!(doc.page_objects(0).unwrap().len(), 1, "rien n'a changé");
}

#[test]
fn set_object_text_validates_text_and_kind() {
    let mut doc = one_page();
    assert!(matches!(
        doc.set_object_text(0, 0, "   "),
        Err(CoreError::InvalidOperation(_))
    ));
    let long = "a".repeat(4097);
    assert!(matches!(
        doc.set_object_text(0, 0, &long),
        Err(CoreError::InvalidOperation(_))
    ));
    // Un objet chemin (trait de signature) n'a pas de texte.
    doc.insert_signature(
        0,
        Rect {
            x: 100.0,
            y: 100.0,
            w: 200.0,
            h: 60.0,
        },
        &SignatureSource::Strokes {
            strokes: vec![vec![[100.0, 110.0], [200.0, 150.0]]],
        },
    )
    .unwrap();
    let objects = doc.page_objects(0).unwrap();
    assert_eq!(objects.len(), 2);
    assert_eq!(objects[1].kind, ObjectKind::Path);
    assert!(objects[1].text.is_none());
    assert!(matches!(
        doc.set_object_text(0, 1, "x"),
        Err(CoreError::InvalidOperation(_))
    ));
    assert_eq!(page_text(&doc, 0), "Page1");
}
