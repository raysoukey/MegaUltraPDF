// SPDX-License-Identifier: GPL-3.0-or-later
//! Identités PKCS#12 et signatures CMS détachées (feature `signature`, Lot 8).
#![cfg(feature = "signature")]

use mega_core::CoreError;
use mega_core::digital_signature::{SignatureStatus, SigningIdentity, cms, sha256};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/");
const WHEN: i64 = 1_793_000_000; // une date fixe (secondes Unix), identique à `/M`

fn p12(name: &str) -> Vec<u8> {
    std::fs::read(format!("{FIXTURES}{name}")).unwrap()
}

fn rsa() -> SigningIdentity {
    SigningIdentity::from_pkcs12(&p12("test-signer.p12"), "test").unwrap()
}

fn ec() -> SigningIdentity {
    SigningIdentity::from_pkcs12(&p12("test-signer-ec.p12"), "test").unwrap()
}

#[test]
fn pkcs12_rsa_and_ec_identities_load() {
    assert_eq!(rsa().signer_name(), "MegaUltraPDF Test Signer");
    assert_eq!(ec().signer_name(), "MegaUltraPDF Test Signer EC");
}

#[test]
fn wrong_pkcs12_password_is_wrong_password() {
    let err = SigningIdentity::from_pkcs12(&p12("test-signer.p12"), "faux").unwrap_err();
    assert!(matches!(err, CoreError::WrongPassword), "{err:?}");
}

#[test]
fn garbage_is_not_a_certificate() {
    let err = SigningIdentity::from_pkcs12(b"ceci n'est pas un PKCS#12", "test").unwrap_err();
    assert!(
        matches!(err, CoreError::InvalidOperation(ref m) if m.contains("certificat")),
        "{err:?}"
    );
}

#[test]
fn cms_round_trip_verifies() {
    let digest = sha256(b"le contenu signe");
    for identity in [rsa(), ec()] {
        let der = cms::sign_digest(&identity, &digest, WHEN).unwrap();
        let check = cms::verify(&der, &digest);
        assert_eq!(
            check.status,
            SignatureStatus::Valid,
            "{}",
            identity.signer_name()
        );
        assert_eq!(
            check.signer.as_deref(),
            Some(identity.signer_name().as_str())
        );
        assert_eq!(check.signed_at, Some(WHEN));
    }
}

#[test]
fn cms_with_another_digest_is_invalid() {
    let der = cms::sign_digest(&rsa(), &sha256(b"original"), WHEN).unwrap();
    let check = cms::verify(&der, &sha256(b"modifie"));
    assert!(
        matches!(check.status, SignatureStatus::Invalid(_)),
        "{check:?}"
    );
}

#[test]
fn cms_followed_by_zero_padding_still_verifies() {
    // Dans un PDF, `/Contents` est plus grand que la signature et complété de zéros.
    let digest = sha256(b"rembourrage");
    let mut der = cms::sign_digest(&ec(), &digest, WHEN).unwrap();
    der.extend_from_slice(&[0u8; 300]);
    assert_eq!(cms::verify(&der, &digest).status, SignatureStatus::Valid);
}

#[test]
fn cms_with_a_damaged_signature_is_invalid() {
    let digest = sha256(b"abime");
    let mut der = cms::sign_digest(&rsa(), &digest, WHEN).unwrap();
    // Les derniers octets sont la valeur de signature RSA.
    let last = der.len() - 1;
    der[last] ^= 0x01;
    assert!(matches!(
        cms::verify(&der, &digest).status,
        SignatureStatus::Invalid(_)
    ));
}

#[test]
fn not_a_cms_is_unsupported_or_invalid_without_panicking() {
    let check = cms::verify(b"\x30\x03\x02\x01\x00", &sha256(b"x"));
    assert!(!matches!(check.status, SignatureStatus::Valid));
}

/// Vérification croisée manuelle par OpenSSL : `MEGA_CMS_SAMPLES=<dossier> cargo test -p
/// mega-core --test digital_signature -- --ignored`, puis
/// `openssl cms -verify -binary -inform DER -in rsa.der -content data.bin -noverify`.
#[test]
#[ignore]
fn write_samples_for_openssl() {
    let dir = std::path::PathBuf::from(std::env::var("MEGA_CMS_SAMPLES").unwrap());
    let data = b"contenu signe pour openssl";
    std::fs::write(dir.join("data.bin"), data).unwrap();
    let digest = sha256(data);
    for (name, identity) in [("rsa", rsa()), ("ec", ec())] {
        let der = cms::sign_digest(&identity, &digest, WHEN).unwrap();
        std::fs::write(dir.join(format!("{name}.der")), der).unwrap();
    }
}
