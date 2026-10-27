// SPDX-License-Identifier: GPL-3.0-or-later
//! Structures CMS (RFC 5652) détachées : signer une empreinte, vérifier une signature.
//!
//! Une signature PDF `/adbe.pkcs7.detached` est un `ContentInfo` de type `signedData` sans
//! contenu encapsulé : seul l'attribut signé `message-digest` relie la signature aux octets
//! du fichier. On signe donc l'empreinte SHA-256 de ces octets, accompagnée des attributs
//! `content-type`, `message-digest` (ajoutés par le constructeur de la crate `cms`) et
//! `signing-time` (ajouté ici, à l'instant écrit aussi dans `/M`).

use std::time::Duration;

use cms::builder::{SignedDataBuilder, SignerInfoBuilder};
use cms::cert::{CertificateChoices, IssuerAndSerialNumber};
use cms::content_info::ContentInfo;
use cms::signed_data::{EncapsulatedContentInfo, SignedData, SignerIdentifier, SignerInfo};
use der::asn1::{OctetString, SetOfVec};
use der::{Decode, Encode};
use spki::AlgorithmIdentifierOwned;
use x509_cert::Certificate;
use x509_cert::attr::{Attribute, AttributeValue};

use super::{SigIssue, SignatureStatus, SigningIdentity, SigningKey};
use crate::error::{CoreError, CoreResult};

const ID_DATA: der::oid::ObjectIdentifier =
    der::oid::ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.1");
const ID_SIGNED_DATA: der::oid::ObjectIdentifier =
    der::oid::ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.2");
const ID_SHA256: der::oid::ObjectIdentifier =
    der::oid::ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");
const ID_MESSAGE_DIGEST: der::oid::ObjectIdentifier =
    der::oid::ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");
const ID_SIGNING_TIME: der::oid::ObjectIdentifier =
    der::oid::ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.5");
const RSA_ENCRYPTION: der::oid::ObjectIdentifier =
    der::oid::ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.1");
const SHA256_WITH_RSA: der::oid::ObjectIdentifier =
    der::oid::ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.11");
const ECDSA_WITH_SHA256: der::oid::ObjectIdentifier =
    der::oid::ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2");
const ID_CONTENT_TYPE: der::oid::ObjectIdentifier =
    der::oid::ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.3");
const ID_SUBJECT_KEY_IDENTIFIER: der::oid::ObjectIdentifier =
    der::oid::ObjectIdentifier::new_unwrap("2.5.29.14");
const COMMON_NAME: der::oid::ObjectIdentifier = der::oid::ObjectIdentifier::new_unwrap("2.5.4.3");

/// Ce que dit une signature CMS une fois vérifiée.
#[derive(Debug, Clone, PartialEq)]
pub struct CmsCheck {
    /// Nom commun du certificat signataire, s'il a pu être lu.
    pub signer: Option<String>,
    /// Attribut `signing-time` (secondes Unix) : déclaré par le signataire, non certifié.
    pub signed_at: Option<i64>,
    /// Verdict.
    pub status: SignatureStatus,
}

fn failed(detail: impl std::fmt::Display) -> CoreError {
    CoreError::InvalidOperation(format!("signature CMS impossible : {detail}"))
}

/// L'attribut `signing-time` (UTCTime jusqu'en 2049, comme l'exige la RFC 5652 §11.3).
fn signing_time_attribute(unix_seconds: i64) -> CoreResult<Attribute> {
    let secs = u64::try_from(unix_seconds).map_err(|_| failed("date antérieure à 1970"))?;
    let when = der::DateTime::from_unix_duration(Duration::from_secs(secs)).map_err(failed)?;
    let value_der = if when.year() <= 2049 {
        der::asn1::UtcTime::from_date_time(when)
            .map_err(failed)?
            .to_der()
    } else {
        der::asn1::GeneralizedTime::from_date_time(when).to_der()
    }
    .map_err(failed)?;
    let mut values = SetOfVec::new();
    values
        .insert(AttributeValue::from_der(&value_der).map_err(failed)?)
        .map_err(failed)?;
    Ok(Attribute {
        oid: ID_SIGNING_TIME,
        values,
    })
}

/// Signe l'empreinte SHA-256 `digest` : retourne le `ContentInfo` CMS encodé en DER, avec
/// le certificat du signataire et la chaîne du fichier PKCS#12.
pub fn sign_digest(
    identity: &SigningIdentity,
    digest: &[u8; 32],
    signing_time: i64,
) -> CoreResult<Vec<u8>> {
    let econtent = EncapsulatedContentInfo {
        econtent_type: ID_DATA,
        econtent: None,
    };
    let sid = SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
        issuer: identity.certificate.tbs_certificate.issuer.clone(),
        serial_number: identity.certificate.tbs_certificate.serial_number.clone(),
    });
    let digest_algorithm = AlgorithmIdentifierOwned {
        oid: ID_SHA256,
        parameters: None,
    };
    let mut builder = SignedDataBuilder::new(&econtent);
    builder
        .add_digest_algorithm(digest_algorithm.clone())
        .map_err(failed)?;
    for cert in &identity.chain {
        builder
            .add_certificate(CertificateChoices::Certificate(cert.clone()))
            .map_err(failed)?;
    }
    let time = signing_time_attribute(signing_time)?;
    // \[Apprentissage\] Même constructeur pour deux types de clés : `add_signer_info` est
    // générique (`S: Signer<Signature>`), et chaque bras du `match` l'instancie avec son type
    // de clé et de signature. Le compilateur produit une version par bras.
    match &identity.key {
        SigningKey::Rsa(key) => {
            let mut info = SignerInfoBuilder::new(
                key.as_ref(),
                sid,
                digest_algorithm,
                &econtent,
                Some(digest.as_slice()),
            )
            .map_err(failed)?;
            info.add_signed_attribute(time).map_err(failed)?;
            builder
                .add_signer_info::<_, rsa::pkcs1v15::Signature>(info)
                .map_err(failed)?;
        }
        SigningKey::Ec(key) => {
            let mut info = SignerInfoBuilder::new(
                key,
                sid,
                digest_algorithm,
                &econtent,
                Some(digest.as_slice()),
            )
            .map_err(failed)?;
            info.add_signed_attribute(time).map_err(failed)?;
            builder
                .add_signer_info::<_, p256::ecdsa::DerSignature>(info)
                .map_err(failed)?;
        }
    }
    let content_info = builder.build().map_err(failed)?;
    content_info.to_der().map_err(failed)
}

/// Un élément DER (longueur définie) qui commence en `pos` : son étiquette, le début de son
/// contenu et sa fin (exclue). `None` s'il déborde de `bytes` ou s'il est mal formé.
fn element(bytes: &[u8], pos: usize) -> Option<(u8, usize, usize)> {
    let tag = *bytes.get(pos)?;
    let first = *bytes.get(pos.checked_add(1)?)?;
    let (header, len) = if first < 0x80 {
        (2, usize::from(first))
    } else {
        let count = usize::from(first & 0x7f);
        if count == 0 || count > 4 {
            return None;
        }
        let mut len = 0usize;
        for i in 0..count {
            len = (len << 8) | usize::from(*bytes.get(pos.checked_add(2 + i)?)?);
        }
        (2 + count, len)
    };
    let start = pos.checked_add(header)?;
    let end = start.checked_add(len)?;
    (end <= bytes.len()).then_some((tag, start, end))
}

/// Position (début, fin) des attributs signés `[0] IMPLICIT` du premier signataire, **tels
/// qu'ils sont écrits** dans la structure CMS : c'est sur ces octets-là (étiquette remplacée
/// par celle d'un SET, `0x31`) que porte la signature (RFC 5652 §5.4). Les décoder puis les
/// réencoder les trierait (règle DER des SET) et fausserait la vérification d'une signature
/// dont les attributs ne sont pas dans l'ordre DER.
fn signed_attrs_span(der: &[u8]) -> Option<(usize, usize)> {
    // ContentInfo ::= SEQUENCE { contentType, [0] EXPLICIT SignedData }
    let (0x30, content_info, _) = element(der, 0)? else {
        return None;
    };
    let (_, _, after_type) = element(der, content_info)?;
    let (0xA0, explicit, _) = element(der, after_type)? else {
        return None;
    };
    // SignedData ::= SEQUENCE { version, digestAlgorithms, encapContentInfo,
    //                           [0] certificates OPTIONAL, [1] crls OPTIONAL, signerInfos }
    let (0x30, signed_data, _) = element(der, explicit)? else {
        return None;
    };
    let mut pos = signed_data;
    for _ in 0..3 {
        pos = element(der, pos)?.2;
    }
    while let Some((0xA0 | 0xA1, _, end)) = element(der, pos) {
        pos = end;
    }
    let (0x31, signer_infos, _) = element(der, pos)? else {
        return None;
    };
    // SignerInfo ::= SEQUENCE { version, sid, digestAlgorithm, [0] signedAttrs, … }
    let (0x30, signer_info, _) = element(der, signer_infos)? else {
        return None;
    };
    let mut pos = signer_info;
    for _ in 0..3 {
        pos = element(der, pos)?.2;
    }
    let (0xA0, _, end) = element(der, pos)? else {
        return None;
    };
    Some((pos, end))
}

/// Nom commun (CN) d'un certificat, sinon son sujet complet.
pub(crate) fn common_name(cert: &Certificate) -> String {
    for rdn in cert.tbs_certificate.subject.0.iter() {
        for atv in rdn.0.iter() {
            if atv.oid == COMMON_NAME {
                if let Ok(s) = atv.value.decode_as::<der::asn1::Utf8StringRef<'_>>() {
                    return s.as_str().to_string();
                }
                if let Ok(s) = atv.value.decode_as::<der::asn1::PrintableStringRef<'_>>() {
                    return s.as_str().to_string();
                }
            }
        }
    }
    cert.tbs_certificate.subject.to_string()
}

fn invalid(issue: SigIssue) -> SignatureStatus {
    SignatureStatus::Invalid(issue)
}

fn unsupported(issue: SigIssue) -> SignatureStatus {
    SignatureStatus::Unsupported(issue)
}

/// Vérifie une signature CMS détachée contre l'empreinte `digest` des octets signés. Ne
/// panique jamais : toute donnée mal formée donne un verdict `Invalid` ou `Unsupported`.
pub fn verify(der: &[u8], digest: &[u8]) -> CmsCheck {
    let mut check = CmsCheck {
        signer: None,
        signed_at: None,
        status: unsupported(SigIssue::CmsUnreadable),
    };
    // Un `/Contents` de PDF est complété de zéros qu'un décodeur DER strict refuserait : on
    // ne garde que l'élément annoncé par l'en-tête.
    let Some((_, _, len)) = element(der, 0) else {
        return check;
    };
    let der = &der[..len];
    let Ok(content_info) = ContentInfo::from_der(der) else {
        return check;
    };
    if content_info.content_type != ID_SIGNED_DATA {
        check.status = unsupported(SigIssue::NotSignedData);
        return check;
    }
    let Ok(signed) = content_info.content.decode_as::<SignedData>() else {
        return check;
    };
    if signed.encap_content_info.econtent.is_some()
        || signed.encap_content_info.econtent_type != ID_DATA
    {
        check.status = unsupported(SigIssue::NotDetached);
        return check;
    }
    // Un seul signataire : en vérifier un et taire les autres serait trompeur.
    if signed.signer_infos.0.len() > 1 {
        check.status = unsupported(SigIssue::MultipleSigners);
        return check;
    }
    let Some(info) = signed.signer_infos.0.iter().next() else {
        check.status = invalid(SigIssue::NoSigner);
        return check;
    };
    let certificate = signer_certificate(&signed, info);
    check.signer = certificate.map(common_name);
    check.signed_at = signing_time(info);
    check.status = match certificate {
        None => invalid(SigIssue::SignerCertificateMissing),
        Some(cert) => verify_signer(der, info, cert, digest),
    };
    check
}

/// Le certificat désigné par l'identifiant du signataire : émetteur et numéro de série, ou
/// identifiant de clé (extension `subjectKeyIdentifier`, comparée exactement). Aucun repli
/// sur « le seul certificat présent » : un identifiant qui ne correspond pas est un échec.
fn signer_certificate<'a>(signed: &'a SignedData, info: &SignerInfo) -> Option<&'a Certificate> {
    match &info.sid {
        SignerIdentifier::IssuerAndSerialNumber(id) => certificates(signed).find(|c| {
            c.tbs_certificate.issuer == id.issuer
                && c.tbs_certificate.serial_number == id.serial_number
        }),
        SignerIdentifier::SubjectKeyIdentifier(ski) => {
            certificates(signed).find(|c| subject_key_identifier(c).as_ref() == Some(ski))
        }
    }
}

/// L'extension `subjectKeyIdentifier` d'un certificat, si elle est présente et lisible.
fn subject_key_identifier(
    cert: &Certificate,
) -> Option<x509_cert::ext::pkix::SubjectKeyIdentifier> {
    cert.tbs_certificate
        .extensions
        .as_ref()?
        .iter()
        .find(|e| e.extn_id == ID_SUBJECT_KEY_IDENTIFIER)
        .and_then(|e| {
            x509_cert::ext::pkix::SubjectKeyIdentifier::from_der(e.extn_value.as_bytes()).ok()
        })
}

fn certificates(signed: &SignedData) -> impl Iterator<Item = &Certificate> {
    signed.certificates.iter().flat_map(|set| {
        set.0.iter().filter_map(|choice| match choice {
            CertificateChoices::Certificate(cert) => Some(cert),
            _ => None,
        })
    })
}

/// Valeur de l'attribut signé `oid`, seulement s'il est présent **une fois** avec **une**
/// valeur : un attribut obligatoire en double rend la signature ambiguë, donc refusée.
fn attribute(info: &SignerInfo, oid: der::oid::ObjectIdentifier) -> Option<&AttributeValue> {
    let mut matching = info.signed_attrs.as_ref()?.iter().filter(|a| a.oid == oid);
    let attr = matching.next()?;
    if matching.next().is_some() || attr.values.len() != 1 {
        return None;
    }
    attr.values.iter().next()
}

fn signing_time(info: &SignerInfo) -> Option<i64> {
    let value = attribute(info, ID_SIGNING_TIME)?;
    let when = value
        .decode_as::<der::asn1::UtcTime>()
        .map(|t| t.to_date_time())
        .or_else(|_| {
            value
                .decode_as::<der::asn1::GeneralizedTime>()
                .map(|t| t.to_date_time())
        })
        .ok()?;
    i64::try_from(when.unix_duration().as_secs()).ok()
}

fn verify_signer(
    der: &[u8],
    info: &SignerInfo,
    cert: &Certificate,
    digest: &[u8],
) -> SignatureStatus {
    if info.digest_alg.oid != ID_SHA256 {
        return unsupported(SigIssue::DigestAlgorithm);
    }
    if info.signed_attrs.is_none() {
        return unsupported(SigIssue::NoSignedAttributes);
    }
    let Some(stated) =
        attribute(info, ID_MESSAGE_DIGEST).and_then(|v| v.decode_as::<OctetString>().ok())
    else {
        return invalid(SigIssue::MessageDigestMissing);
    };
    if stated.as_bytes() != digest {
        return invalid(SigIssue::ContentModified);
    }
    // `content-type` est obligatoire (RFC 5652 §5.3) et doit désigner des données (`id-data`) :
    // sinon une signature faite pour un autre usage pourrait être réemployée ici.
    let content_type = attribute(info, ID_CONTENT_TYPE)
        .and_then(|v| v.decode_as::<der::oid::ObjectIdentifier>().ok());
    if content_type != Some(ID_DATA) {
        return invalid(SigIssue::ContentTypeInvalid);
    }
    // Contrôle local de la date déclarée : dans la période de validité du certificat.
    if let Some(when) = signing_time(info) {
        let validity = &cert.tbs_certificate.validity;
        let secs = |t: &x509_cert::time::Time| i64::try_from(t.to_unix_duration().as_secs()).ok();
        let inside = matches!(
            (secs(&validity.not_before), secs(&validity.not_after)),
            (Some(from), Some(to)) if from <= when && when <= to
        );
        if !inside {
            return invalid(SigIssue::SigningTimeOutsideValidity);
        }
    }
    // Ce qui est signé : les attributs signés **tels qu'écrits**, étiquette d'un SET (0x31).
    let Some((start, end)) = signed_attrs_span(der) else {
        return unsupported(SigIssue::SignedAttributesUnreadable);
    };
    let mut message = der[start..end].to_vec();
    message[0] = 0x31;
    let Ok(spki_der) = cert.tbs_certificate.subject_public_key_info.to_der() else {
        return invalid(SigIssue::PublicKeyUnreadable);
    };
    let signature = info.signature.as_bytes();
    let algorithm = info.signature_algorithm.oid;
    if algorithm == RSA_ENCRYPTION || algorithm == SHA256_WITH_RSA {
        use rsa::pkcs8::DecodePublicKey;
        use rsa::signature::Verifier;
        let Ok(public) = rsa::RsaPublicKey::from_public_key_der(&spki_der) else {
            return invalid(SigIssue::PublicKeyUnreadable);
        };
        if rsa::traits::PublicKeyParts::size(&public) * 8 < 2048 {
            return unsupported(SigIssue::WeakKey);
        }
        let Ok(sig) = rsa::pkcs1v15::Signature::try_from(signature) else {
            return invalid(SigIssue::SignatureValueUnreadable);
        };
        match rsa::pkcs1v15::VerifyingKey::<sha2::Sha256>::new(public).verify(&message, &sig) {
            Ok(()) => SignatureStatus::Intact,
            Err(_) => invalid(SigIssue::SignatureMismatch),
        }
    } else if algorithm == ECDSA_WITH_SHA256 {
        use p256::ecdsa::signature::Verifier;
        use p256::pkcs8::DecodePublicKey;
        let Ok(public) = p256::ecdsa::VerifyingKey::from_public_key_der(&spki_der) else {
            return unsupported(SigIssue::UnsupportedCurve);
        };
        let Ok(sig) = p256::ecdsa::DerSignature::try_from(signature) else {
            return invalid(SigIssue::SignatureValueUnreadable);
        };
        match public.verify(&message, &sig) {
            Ok(()) => SignatureStatus::Intact,
            Err(_) => invalid(SigIssue::SignatureMismatch),
        }
    } else {
        unsupported(SigIssue::SignatureAlgorithm)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::digital_signature::{SigningIdentity, SigningKey};
    use rsa::signature::{SignatureEncoding, Signer};

    /// Une signature tierce dont les attributs signés ne sont **pas** dans l'ordre DER (le
    /// signataire a signé les octets tels qu'il les a écrits) reste intègre.
    #[test]
    fn signed_attributes_are_verified_on_their_original_bytes() {
        let p12 = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/test-signer.p12"
        ))
        .unwrap();
        let identity = SigningIdentity::from_pkcs12(&p12, "test").unwrap();
        let SigningKey::Rsa(key) = &identity.key else {
            panic!("clé RSA attendue");
        };
        let digest = [7u8; 32];
        let mut der = sign_digest(&identity, &digest, 1_800_000_000).unwrap();
        let old_signature = {
            let info = ContentInfo::from_der(&der).unwrap();
            let signed = info.content.decode_as::<SignedData>().unwrap();
            let signer = signed.signer_infos.0.iter().next().unwrap().clone();
            signer.signature.as_bytes().to_vec()
        };
        // Les attributs, dans l'ordre inverse : même longueur, ordre non DER.
        let (start, end) = signed_attrs_span(&der).unwrap();
        let (_, content, _) = element(&der, start).unwrap();
        let mut attributes = Vec::new();
        let mut pos = content;
        while pos < end {
            let (_, _, next) = element(&der, pos).unwrap();
            attributes.push(der[pos..next].to_vec());
            pos = next;
        }
        assert_eq!(attributes.len(), 3);
        attributes.reverse();
        der[content..end].copy_from_slice(&attributes.concat());
        // Nouvelle signature des octets tels qu'écrits (étiquette d'un SET).
        let mut message = der[start..end].to_vec();
        message[0] = 0x31;
        let new_signature = key.sign(&message).to_vec();
        let at = der
            .windows(old_signature.len())
            .position(|w| w == old_signature.as_slice())
            .unwrap();
        der[at..at + new_signature.len()].copy_from_slice(&new_signature);
        assert_eq!(verify(&der, &digest).status, SignatureStatus::Intact);
    }
}
