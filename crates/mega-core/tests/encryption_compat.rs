// SPDX-License-Identifier: GPL-3.0-or-later
//! Compatibilité des chiffrements (Lot 13, audit C8 à C10) : les variantes produites par
//! d'autres logiciels (qpdf ici) s'ouvrent, se modifient et s'enregistrent **en gardant leur
//! protection** ; un chiffrement par certificat est refusé avec un message clair ; un mot de
//! passe Unicode fonctionne.

use mega_core::edit::EditCommand;
use mega_core::overlay::Watermark;
use mega_core::{Color, CoreError, Document, Permissions, Protection};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../fixtures/chiffrement/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn watermark() -> EditCommand {
    EditCommand::Watermark {
        spec: Watermark {
            text: "Modifié".to_string(),
            size: 40.0,
            opacity: 0.4,
            angle_deg: 30.0,
            color: Color { r: 0, g: 0, b: 200 },
            pages: None,
        },
    }
}

/// Ouvre avec le mot de passe utilisateur et avec le propriétaire, modifie, enregistre :
/// la sortie demande toujours un mot de passe et s'ouvre avec le même.
fn round_trip(name: &str, user: &str, owner: &str) {
    let plain = std::fs::read(format!(
        "{}/../../fixtures/simple.pdf",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let expected = Document::from_bytes(plain, None)
        .unwrap()
        .page_text(0)
        .unwrap();
    assert!(!expected.trim().is_empty());
    assert!(matches!(
        Document::from_bytes(fixture(name), None),
        Err(CoreError::PasswordRequired)
    ));
    let by_owner = Document::from_bytes(fixture(name), Some(owner)).unwrap();
    assert_eq!(by_owner.page_text(0).unwrap(), expected, "{name}");
    let mut doc = Document::from_bytes(fixture(name), Some(user)).unwrap();
    assert_eq!(doc.page_text(0).unwrap(), expected, "{name}");
    doc.apply(&watermark()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("sortie.pdf");
    doc.save_as(&out).unwrap();
    let bytes = std::fs::read(&out).unwrap();
    assert!(
        matches!(
            Document::from_bytes(bytes.clone(), None),
            Err(CoreError::PasswordRequired)
        ),
        "{name} : protection perdue à l'enregistrement"
    );
    let reopened = Document::from_bytes(bytes, Some(user)).unwrap();
    assert!(reopened.page_text(0).unwrap().contains("Modifié"), "{name}");
}

#[test]
fn rc4_40_bits_round_trips() {
    round_trip("rc4-40.pdf", "u", "o");
}

#[test]
fn aes_128_with_distinct_passwords_round_trips() {
    round_trip("aes128.pdf", "u", "o");
}

#[test]
fn aes_256_revision_5_round_trips() {
    round_trip("aes256-r5.pdf", "u", "o");
}

/// C10 : un mot de passe accentué et chinois (révision 6 : SASLprep puis UTF-8).
#[test]
fn a_unicode_password_opens_a_revision_6_file() {
    round_trip("mot-de-passe-unicode.pdf", "été密码", "o");
}

/// C10 : protéger avec un mot de passe hors Latin-1 ; il rouvre le fichier.
#[test]
fn a_document_can_be_protected_with_a_unicode_password() {
    let mut doc = Document::from_bytes(mega_core::testing::sample_pdf(1, "unicode"), None).unwrap();
    doc.protect(&Protection {
        user_password: "Пароль-密码-é".to_string(),
        owner_password: "propriétaire ✓".to_string(),
        permissions: Permissions {
            print: true,
            copy: true,
            modify: true,
            annotate: true,
        },
    })
    .unwrap();
    let bytes = doc.to_bytes().unwrap();
    assert!(matches!(
        Document::from_bytes(bytes.clone(), None),
        Err(CoreError::PasswordRequired)
    ));
    assert!(Document::from_bytes(bytes.clone(), Some("Пароль-密码-é")).is_ok());
    assert!(Document::from_bytes(bytes, Some("propriétaire ✓")).is_ok());
}

/// C9 : un chiffrement par certificat ne s'ouvre avec aucun mot de passe : le dire, au lieu
/// de demander un mot de passe en boucle.
#[test]
fn a_certificate_encrypted_file_is_refused_with_a_clear_error() {
    for password in [None, Some("n'importe quoi")] {
        let result = Document::from_bytes(fixture("certificat.pdf"), password);
        assert!(
            matches!(result, Err(CoreError::UnsupportedEncryption(ref m)) if m.contains("certificat")),
            "{:?}",
            result.err()
        );
    }
}
