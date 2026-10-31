// SPDX-License-Identifier: GPL-3.0-or-later
//! Lot 10 : les réécritures `lopdf` (nettoyage d'enregistrement, filigrane, caviardage…)
//! traitent aussi les documents chiffrés, qu'elles **rechiffrent à l'identique** : même clé,
//! mêmes `/O`, `/U`, `/P`, même `/ID`, donc même mot de passe et mêmes permissions.

use std::collections::BTreeMap;
use std::sync::Arc;

use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
use lopdf::encryption::{EncryptionState, EncryptionVersion, Permissions};
use mega_core::cleanup::before_save;
use mega_core::overlay::Watermark;
use mega_core::security::Protection;
use mega_core::testing::sample_pdf;
use mega_core::{Color, CoreError, Document, Rect};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// `/ModDate` du dictionnaire `/Info`, lu par lopdf avec le mot de passe (`None` s'il manque).
fn mod_date(bytes: &[u8], password: &str) -> Option<String> {
    let doc =
        lopdf::Document::load_mem_with_options(bytes, lopdf::LoadOptions::with_password(password))
            .unwrap();
    let info = doc.trailer.get(b"Info").ok()?.as_reference().ok()?;
    let date = doc.get_dictionary(info).ok()?.get(b"ModDate").ok()?;
    Some(String::from_utf8_lossy(date.as_str().ok()?).into_owned())
}

fn is_encrypted(bytes: &[u8]) -> bool {
    lopdf::Document::load_mem(bytes)
        .map(|d| d.is_encrypted() || d.was_encrypted())
        .unwrap_or(false)
}

#[test]
fn encrypted_document_is_cleaned_and_stays_encrypted() {
    let bytes = fixture("encrypted.pdf");
    let out = before_save(&bytes, Some("secret"), None).unwrap();
    assert_ne!(out, bytes, "le document a été réécrit (nettoyé)");
    assert!(mod_date(&out, "secret").is_some(), "/ModDate posé");
    assert!(is_encrypted(&out));
    assert!(matches!(
        Document::from_bytes(out.clone(), None),
        Err(CoreError::PasswordRequired)
    ));
    let doc = Document::from_bytes(out, Some("secret")).unwrap();
    assert_eq!(doc.page_count(), 3);
    assert!(doc.page_text(0).unwrap().contains("Page 1"));
}

#[test]
fn owner_only_document_is_cleaned_and_stays_protected() {
    let bytes = fixture("owner-only.pdf");
    let out = before_save(&bytes, None, None).unwrap();
    assert_ne!(out, bytes, "réécrit");
    assert!(mod_date(&out, "").is_some(), "/ModDate posé");
    assert!(
        String::from_utf8_lossy(&out).contains("/Encrypt"),
        "protection conservée"
    );
    assert!(Document::from_bytes(out, None).is_ok());
}

/// RF3 : AES-256 (révision 6) avec permissions restreintes : même mot de passe, même `/P`,
/// et le titre n'apparaît pas en clair.
#[test]
fn aes256_cleanup_keeps_password_and_permissions() {
    let mut doc = Document::from_bytes(sample_pdf(1, "TitreConfidentiel"), None).unwrap();
    let protection = Protection {
        user_password: "u".to_string(),
        owner_password: "o".to_string(),
        permissions: mega_core::Permissions {
            print: true,
            copy: false,
            modify: false,
            annotate: true,
        },
    };
    doc.protect(&protection).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("protege.pdf");
    doc.save_as(&path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let before = Document::from_bytes(bytes.clone(), Some("u"))
        .unwrap()
        .security()
        .unwrap();
    let out = before_save(&bytes, Some("u"), None).unwrap();
    assert!(mod_date(&out, "u").is_some(), "nettoyé");
    assert!(!String::from_utf8_lossy(&out).contains("TitreConfidentiel"));
    let reopened = Document::from_bytes(out, Some("u")).unwrap();
    assert_eq!(reopened.security().unwrap().permissions, before.permissions);
    assert!(!reopened.security().unwrap().permissions.copy);
}

/// Chiffre `sample_pdf` par lopdf en RC4 128 bits (V2) ou AES-128 (V4).
fn legacy_encrypted(aes: bool) -> Vec<u8> {
    let mut doc = lopdf::Document::load_mem(&sample_pdf(1, "ancien")).unwrap();
    if doc.trailer.get(b"ID").is_err() {
        let id = lopdf::Object::String(
            b"0123456789abcdef".to_vec(),
            lopdf::StringFormat::Hexadecimal,
        );
        doc.trailer
            .set("ID", lopdf::Object::Array(vec![id.clone(), id]));
    }
    let state = if aes {
        let mut filters: BTreeMap<Vec<u8>, Arc<dyn CryptFilter>> = BTreeMap::new();
        filters.insert(b"StdCF".to_vec(), Arc::new(Aes128CryptFilter));
        EncryptionState::try_from(EncryptionVersion::V4 {
            document: &doc,
            encrypt_metadata: true,
            crypt_filters: filters,
            stream_filter: b"StdCF".to_vec(),
            string_filter: b"StdCF".to_vec(),
            owner_password: "o",
            user_password: "u",
            permissions: Permissions::all(),
        })
        .unwrap()
    } else {
        EncryptionState::try_from(EncryptionVersion::V2 {
            document: &doc,
            owner_password: "o",
            user_password: "u",
            key_length: 128,
            permissions: Permissions::all(),
        })
        .unwrap()
    };
    doc.encrypt(&state).unwrap();
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

#[test]
fn rc4_and_aes128_documents_are_cleaned_and_reopen() {
    for aes in [false, true] {
        let bytes = legacy_encrypted(aes);
        assert!(
            Document::from_bytes(bytes.clone(), Some("u")).is_ok(),
            "fixture lisible"
        );
        let out = before_save(&bytes, Some("u"), None).unwrap();
        assert!(mod_date(&out, "u").is_some(), "aes={aes} : nettoyé");
        let doc = Document::from_bytes(out, Some("u")).unwrap();
        assert!(doc.page_text(0).unwrap().contains("Page 1"), "aes={aes}");
    }
}

#[test]
fn watermark_on_encrypted_document() {
    let mut doc = Document::from_bytes(fixture("encrypted.pdf"), Some("secret")).unwrap();
    doc.add_watermark(&Watermark {
        text: "CONFIDENTIEL".to_string(),
        size: 40.0,
        opacity: 0.5,
        angle_deg: 30.0,
        color: Color { r: 200, g: 0, b: 0 },
        pages: Some(vec![0]),
    })
    .unwrap();
    assert!(doc.page_text(0).unwrap().contains("CONFIDENTIEL"));
    let bytes = doc.to_bytes().unwrap();
    assert!(is_encrypted(&bytes), "toujours chiffré");
    let again = Document::from_bytes(bytes, Some("secret")).unwrap();
    assert!(again.page_text(0).unwrap().contains("CONFIDENTIEL"));
}

#[test]
fn redact_on_encrypted_document_drops_the_text() {
    let mut doc = Document::from_bytes(fixture("encrypted.pdf"), Some("secret")).unwrap();
    let report = doc
        .redact(
            0,
            Rect {
                x: 60.0,
                y: 740.0,
                w: 200.0,
                h: 50.0,
            },
        )
        .unwrap();
    assert_eq!(report.texts, 1);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("caviarde.pdf");
    doc.save_as(&path).unwrap();
    let saved = std::fs::read(&path).unwrap();
    assert!(is_encrypted(&saved));
    let reopened = Document::from_bytes(saved, Some("secret")).unwrap();
    assert!(!reopened.page_text(0).unwrap().contains("Page 1"));
}

/// `form_pdf` (ou tout autre PDF) protégé par le mot de passe d'ouverture « u ».
fn protected(bytes: Vec<u8>) -> Vec<u8> {
    let mut doc = Document::from_bytes(bytes, None).unwrap();
    doc.protect(&Protection {
        user_password: "u".to_string(),
        owner_password: "o".to_string(),
        permissions: mega_core::Permissions {
            print: true,
            copy: true,
            modify: true,
            annotate: true,
        },
    })
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("protege.pdf");
    doc.save_as(&path).unwrap();
    std::fs::read(&path).unwrap()
}

#[test]
fn form_of_encrypted_document_is_filled() {
    let bytes = protected(mega_core::testing::form_pdf());
    let mut doc = Document::from_bytes(bytes, Some("u")).unwrap();
    doc.set_field_value(
        0,
        0,
        &mega_core::FieldValue::Text {
            text: "Dupont".to_string(),
        },
    )
    .unwrap();
    let out = doc.to_bytes().unwrap();
    assert!(is_encrypted(&out));
    let reopened = Document::from_bytes(out, Some("u")).unwrap();
    assert_eq!(
        reopened.form_fields(0).unwrap()[0].value.as_deref(),
        Some("Dupont")
    );
}

#[test]
fn metadata_of_encrypted_document_is_edited() {
    let bytes = protected(sample_pdf(1, "ancien titre"));
    let mut doc = Document::from_bytes(bytes, Some("u")).unwrap();
    doc.set_metadata(&mega_core::MetadataPatch {
        title: Some("Nouveau titre".to_string()),
        author: None,
        subject: None,
        keywords: None,
    })
    .unwrap();
    let out = doc.to_bytes().unwrap();
    assert!(is_encrypted(&out));
    let reopened = Document::from_bytes(out, Some("u")).unwrap();
    assert_eq!(reopened.title().as_deref(), Some("Nouveau titre"));
}

/// Relecture PDF du Lot 10 (C1) : un fichier dont seules les pièces jointes sont chiffrées
/// (`/StmF /Identity`) n'est pas « déchiffré » en RC4 par lopdf : le nettoyage le laisse tel
/// que PDFium l'a produit, et le contenu reste lisible.
#[test]
fn attachments_only_encryption_is_not_corrupted() {
    let bytes = mega_core::testing::attachments_only_encrypted_pdf();
    let doc = Document::from_bytes(bytes.clone(), None).unwrap();
    assert!(
        doc.page_text(0).unwrap().contains("Hello Identity"),
        "fixture lisible"
    );
    let out = before_save(&bytes, None, None).unwrap();
    let reopened = Document::from_bytes(out, None).unwrap();
    assert!(reopened.page_text(0).unwrap().contains("Hello Identity"));
    assert_eq!(reopened.title().as_deref(), Some("Plain title"));
    // Une passe lopdf (filigrane) passe désormais par la session « passe-plat » (audit L16).
    let mut doc = Document::from_bytes(bytes, None).unwrap();
    doc.add_watermark(&Watermark {
        text: "X".to_string(),
        size: 40.0,
        opacity: 0.5,
        angle_deg: 0.0,
        color: Color { r: 0, g: 0, b: 0 },
        pages: None,
    })
    .unwrap();
    let again = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    assert!(again.page_text(0).unwrap().contains("Hello Identity"));
    assert_eq!(again.title().as_deref(), Some("Plain title"));
}

/// Octets bruts du flux `/EF /F` de la première pièce jointe. Le fichier est lu **sans**
/// déchiffrement (`/Encrypt` rebaptisé, même longueur) : lopdf appliquerait sinon RC4 aux
/// flux d'un fichier `/StmF /Identity`.
fn without_encrypt(bytes: &[u8]) -> lopdf::Document {
    let mut raw = bytes.to_vec();
    let hits: Vec<usize> = (0..raw.len().saturating_sub(9))
        .filter(|&i| raw[i..].starts_with(b"/Encrypt "))
        .collect();
    for at in hits {
        raw[at + 7] = b'x';
    }
    lopdf::Document::load_mem(&raw).unwrap()
}

fn embedded_file_stream_bytes(bytes: &[u8]) -> Vec<u8> {
    let doc = without_encrypt(bytes);
    let dict_at = |obj: &lopdf::Object| doc.dereference(obj).unwrap().1.clone();
    let root = dict_at(doc.trailer.get(b"Root").unwrap());
    let names = dict_at(root.as_dict().unwrap().get(b"Names").unwrap());
    let embedded = dict_at(names.as_dict().unwrap().get(b"EmbeddedFiles").unwrap());
    let list = embedded
        .as_dict()
        .unwrap()
        .get(b"Names")
        .unwrap()
        .as_array()
        .unwrap();
    let spec = dict_at(&list[1]);
    let ef = dict_at(spec.as_dict().unwrap().get(b"EF").unwrap());
    let file = dict_at(ef.as_dict().unwrap().get(b"F").unwrap());
    file.as_stream().unwrap().content.clone()
}

/// Audit L16 : un fichier dont seules les pièces jointes sont chiffrées se filigrane et se
/// réécrit ; son `/Encrypt`, son `/ID` et les octets (chiffrés) de la pièce jointe restent
/// ceux d'origine.
#[test]
fn identity_filtered_file_keeps_its_encrypt_dictionary_and_attachment_bytes_after_watermark() {
    let input = mega_core::testing::attachments_only_encrypted_with_file_pdf();
    let ef_before = embedded_file_stream_bytes(&input);
    assert_eq!(ef_before, b"CIPHERTEXT-ABC123");
    let mut doc = Document::from_bytes(input.clone(), None).unwrap();
    doc.add_watermark(&Watermark {
        text: "x".to_string(),
        size: 40.0,
        opacity: 0.5,
        angle_deg: 0.0,
        color: Color { r: 0, g: 0, b: 0 },
        pages: None,
    })
    .unwrap();
    let pdfium_bytes = doc.to_bytes().unwrap();
    let out = before_save(&pdfium_bytes, None, None).unwrap();
    assert_eq!(embedded_file_stream_bytes(&out), ef_before);
    assert!(out.windows(9).any(|w| w == b"/Encrypt "));
    let id_of = |bytes: &[u8]| without_encrypt(bytes).trailer.get(b"ID").unwrap().clone();
    // PDFium écrit lui-même un nouveau second élément de `/ID` : la passe ne doit rien changer.
    assert_eq!(id_of(&out), id_of(&pdfium_bytes));
    assert_eq!(
        id_of(&out).as_array().unwrap()[0],
        id_of(&input).as_array().unwrap()[0]
    );
    let reopened = Document::from_bytes(out, None).unwrap();
    assert!(reopened.page_text(0).unwrap().contains("Hello Identity"));
}
