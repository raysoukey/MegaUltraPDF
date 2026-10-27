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

use super::{SignatureStatus, SigningIdentity, SigningKey};
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

/// Longueur DER d'un élément (en-tête compris) lue dans son en-tête : un `/Contents` de PDF
/// est complété de zéros qu'un décodeur DER strict refuserait.
fn der_length(bytes: &[u8]) -> Option<usize> {
    let first = *bytes.get(1)?;
    if first < 0x80 {
        return Some(2 + usize::from(first));
    }
    let count = usize::from(first & 0x7f);
    if count == 0 || count > 4 {
        return None;
    }
    let mut len = 0usize;
    for i in 0..count {
        len = (len << 8) | usize::from(*bytes.get(2 + i)?);
    }
    Some(2 + count + len)
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

fn invalid(detail: &str) -> SignatureStatus {
    SignatureStatus::Invalid(detail.to_string())
}

fn unsupported(detail: &str) -> SignatureStatus {
    SignatureStatus::Unsupported(detail.to_string())
}

/// Vérifie une signature CMS détachée contre l'empreinte `digest` des octets signés. Ne
/// panique jamais : toute donnée mal formée donne un verdict `Invalid` ou `Unsupported`.
pub fn verify(der: &[u8], digest: &[u8]) -> CmsCheck {
    let mut check = CmsCheck {
        signer: None,
        signed_at: None,
        status: unsupported("structure CMS illisible"),
    };
    let Some(len) = der_length(der).filter(|l| *l <= der.len()) else {
        return check;
    };
    let Ok(content_info) = ContentInfo::from_der(&der[..len]) else {
        return check;
    };
    if content_info.content_type != ID_SIGNED_DATA {
        check.status = unsupported("le contenu CMS n'est pas une signature (signedData)");
        return check;
    }
    let Ok(signed) = content_info.content.decode_as::<SignedData>() else {
        return check;
    };
    if signed.encap_content_info.econtent.is_some() {
        check.status = unsupported("signature avec contenu encapsulé (non détachée)");
        return check;
    }
    let Some(info) = signed.signer_infos.0.iter().next() else {
        check.status = invalid("aucun signataire");
        return check;
    };
    let certificate = signer_certificate(&signed, info);
    check.signer = certificate.map(common_name);
    check.signed_at = signing_time(info);
    check.status = match certificate {
        None => invalid("certificat du signataire absent"),
        Some(cert) => verify_signer(info, cert, digest),
    };
    check
}

/// Le certificat désigné par l'identifiant du signataire (émetteur et numéro de série).
fn signer_certificate<'a>(signed: &'a SignedData, info: &SignerInfo) -> Option<&'a Certificate> {
    let SignerIdentifier::IssuerAndSerialNumber(id) = &info.sid else {
        // Identifiant par clé (SKI) : on se contente du seul certificat, s'il est unique.
        let certs: Vec<&Certificate> = certificates(signed).collect();
        return (certs.len() == 1).then(|| certs[0]);
    };
    certificates(signed).find(|c| {
        c.tbs_certificate.issuer == id.issuer && c.tbs_certificate.serial_number == id.serial_number
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

/// Valeur de l'attribut signé `oid`, s'il est présent une fois.
fn attribute(info: &SignerInfo, oid: der::oid::ObjectIdentifier) -> Option<&AttributeValue> {
    info.signed_attrs
        .as_ref()?
        .iter()
        .find(|a| a.oid == oid)
        .and_then(|a| a.values.iter().next())
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

fn verify_signer(info: &SignerInfo, cert: &Certificate, digest: &[u8]) -> SignatureStatus {
    if info.digest_alg.oid != ID_SHA256 {
        return unsupported("algorithme d'empreinte autre que SHA-256");
    }
    let Some(signed_attrs) = info.signed_attrs.as_ref() else {
        return unsupported("signature sans attributs signés");
    };
    let Some(stated) =
        attribute(info, ID_MESSAGE_DIGEST).and_then(|v| v.decode_as::<OctetString>().ok())
    else {
        return invalid("empreinte signée absente");
    };
    if stated.as_bytes() != digest {
        return invalid("le contenu a été modifié depuis la signature");
    }
    // Ce qui est signé : les attributs signés encodés comme un SET (étiquette 0x31).
    let Ok(message) = signed_attrs.to_der() else {
        return invalid("attributs signés illisibles");
    };
    let Ok(spki_der) = cert.tbs_certificate.subject_public_key_info.to_der() else {
        return invalid("clé publique illisible");
    };
    let signature = info.signature.as_bytes();
    let algorithm = info.signature_algorithm.oid;
    if algorithm == RSA_ENCRYPTION || algorithm == SHA256_WITH_RSA {
        use rsa::pkcs8::DecodePublicKey;
        use rsa::signature::Verifier;
        let Ok(public) = rsa::RsaPublicKey::from_public_key_der(&spki_der) else {
            return invalid("clé publique RSA illisible");
        };
        let Ok(sig) = rsa::pkcs1v15::Signature::try_from(signature) else {
            return invalid("valeur de signature RSA illisible");
        };
        match rsa::pkcs1v15::VerifyingKey::<sha2::Sha256>::new(public).verify(&message, &sig) {
            Ok(()) => SignatureStatus::Valid,
            Err(_) => invalid("signature RSA fausse"),
        }
    } else if algorithm == ECDSA_WITH_SHA256 {
        use p256::ecdsa::signature::Verifier;
        use p256::pkcs8::DecodePublicKey;
        let Ok(public) = p256::ecdsa::VerifyingKey::from_public_key_der(&spki_der) else {
            return unsupported("clé ECDSA autre que P-256");
        };
        let Ok(sig) = p256::ecdsa::DerSignature::try_from(signature) else {
            return invalid("valeur de signature ECDSA illisible");
        };
        match public.verify(&message, &sig) {
            Ok(()) => SignatureStatus::Valid,
            Err(_) => invalid("signature ECDSA fausse"),
        }
    } else {
        unsupported("algorithme de signature non pris en charge")
    }
}
