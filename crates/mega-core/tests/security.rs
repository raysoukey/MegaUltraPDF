// SPDX-License-Identifier: GPL-3.0-or-later
//! Protection par mot de passe (AES-256 via `lopdf`) et retrait de la protection.

use mega_core::edit::EditCommand;
use mega_core::security::{Permissions, Protection};
use mega_core::testing::sample_pdf;
use mega_core::{CoreError, Document};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn all() -> Permissions {
    Permissions {
        print: true,
        copy: true,
        modify: true,
        annotate: true,
    }
}

fn protection(user: &str, owner: &str, permissions: Permissions) -> Protection {
    Protection {
        user_password: user.to_string(),
        owner_password: owner.to_string(),
        permissions,
    }
}

#[test]
fn unprotected_document_reports_no_security() {
    let doc = Document::from_bytes(sample_pdf(1, "libre"), None).unwrap();
    let info = doc.security().unwrap();
    assert!(!info.encrypted && !info.user_password_required);
    assert_eq!(info.permissions, all());
}

#[test]
fn protect_then_reopen_requires_password_and_keeps_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("protege.pdf");
    std::fs::write(&path, sample_pdf(2, "secret")).unwrap();
    let mut doc = Document::open(&path, None).unwrap();
    let before = doc.render_page(0, 0.5).unwrap();
    doc.protect(&protection(
        "secret",
        "patron",
        Permissions {
            print: false,
            copy: false,
            modify: true,
            annotate: true,
        },
    ))
    .unwrap();
    // Le document reste ouvert et navigable.
    assert_eq!(doc.page_count(), 2);
    assert_eq!(doc.render_page(0, 0.5).unwrap().rgba, before.rgba);
    let info = doc.security().unwrap();
    assert!(info.encrypted && info.user_password_required);
    assert!(!info.permissions.print && !info.permissions.copy);
    doc.save().unwrap();
    assert!(matches!(
        Document::open(&path, None),
        Err(CoreError::PasswordRequired)
    ));
    assert!(matches!(
        Document::open(&path, Some("faux")),
        Err(CoreError::WrongPassword)
    ));
    let reopened = Document::open(&path, Some("secret")).unwrap();
    assert_eq!(reopened.page_count(), 2);
    let info = reopened.security().unwrap();
    assert!(info.encrypted && !info.permissions.print && info.permissions.modify);
    assert!(String::from_utf8_lossy(&std::fs::read(&path).unwrap()).contains("/AESV3"));
}

#[test]
fn protect_refuses_bad_passwords_and_double_protection() {
    let mut doc = Document::from_bytes(sample_pdf(1, "p"), None).unwrap();
    for (user, owner) in [("", ""), (&"x".repeat(200), "o"), ("🙂", "o")] {
        assert!(
            matches!(
                doc.protect(&protection(user, owner, all())),
                Err(CoreError::InvalidOperation(_))
            ),
            "{user:?}"
        );
    }
    assert!(!doc.security().unwrap().encrypted, "rien n'a changé");
    doc.protect(&protection("", "proprio", all())).unwrap();
    assert!(doc.security().unwrap().encrypted);
    assert!(
        !doc.security().unwrap().user_password_required,
        "mot de passe propriétaire seul : s'ouvre sans mot de passe"
    );
    assert!(matches!(
        doc.protect(&protection("a", "b", all())),
        Err(CoreError::InvalidOperation(_))
    ));
}

#[test]
fn protect_is_undoable() {
    let mut doc = Document::from_bytes(sample_pdf(1, "undo"), None).unwrap();
    doc.apply(&EditCommand::Protect {
        protection: protection("secret", "patron", all()),
    })
    .unwrap();
    assert_eq!(doc.undo_label().as_deref(), Some("Protéger le document"));
    assert!(doc.security().unwrap().encrypted);
    doc.undo().unwrap();
    assert!(!doc.security().unwrap().encrypted);
    doc.redo().unwrap();
    assert!(doc.security().unwrap().encrypted);
    assert_eq!(doc.page_count(), 1);
}

#[test]
fn unprotect_owner_only_needs_the_owner_password() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("owner.pdf");
    std::fs::write(&path, fixture("owner-only.pdf")).unwrap();
    // Ouvert sans mot de passe : on ne peut pas prouver qu'on est le propriétaire.
    let mut doc = Document::open(&path, None).unwrap();
    let info = doc.security().unwrap();
    assert!(info.encrypted && !info.user_password_required);
    assert!(matches!(doc.unprotect(), Err(CoreError::WrongPassword)));
    assert!(doc.security().unwrap().encrypted);
    // Ouvert avec le mot de passe propriétaire : la protection tombe.
    let mut doc = Document::open(&path, Some("secret")).unwrap();
    doc.apply(&EditCommand::Unprotect).unwrap();
    assert_eq!(doc.undo_label().as_deref(), Some("Retirer la protection"));
    assert!(!doc.security().unwrap().encrypted);
    doc.save().unwrap();
    let saved = std::fs::read(&path).unwrap();
    assert!(!String::from_utf8_lossy(&saved).contains("/Encrypt"));
    assert_eq!(Document::open(&path, None).unwrap().page_count(), 3);
}

#[test]
fn unprotect_user_password_document() {
    let mut doc = Document::from_bytes(fixture("encrypted.pdf"), Some("secret")).unwrap();
    assert!(doc.security().unwrap().user_password_required);
    doc.unprotect().unwrap();
    let info = doc.security().unwrap();
    assert!(!info.encrypted && !info.user_password_required);
    let reopened = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    assert_eq!(reopened.page_count(), 3);
    assert!(matches!(
        reopened.security(),
        Ok(i) if !i.encrypted
    ));
    let mut plain = Document::from_bytes(sample_pdf(1, "x"), None).unwrap();
    assert!(matches!(
        plain.unprotect(),
        Err(CoreError::InvalidOperation(_))
    ));
}

#[test]
fn edits_after_protect_are_refused_by_lopdf_passes() {
    let mut doc = Document::from_bytes(sample_pdf(1, "après"), None).unwrap();
    doc.protect(&protection("secret", "patron", all())).unwrap();
    let spec = mega_core::overlay::Watermark {
        text: "X".to_string(),
        size: 40.0,
        opacity: 0.5,
        angle_deg: 0.0,
        color: mega_core::Color { r: 0, g: 0, b: 0 },
        pages: None,
    };
    assert!(matches!(
        doc.add_watermark(&spec),
        Err(CoreError::InvalidOperation(_))
    ));
    // Les éditions PDFium pures (annotations, pages) restent possibles.
    doc.rotate_pages(&[0], 1).unwrap();
    assert_eq!(doc.page_count(), 1);
}
