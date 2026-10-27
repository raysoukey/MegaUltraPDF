// SPDX-License-Identifier: GPL-3.0-or-later
//! Signature numérique cryptographique (feature `signature`, Lot 8).
//!
//! Une **identité** (certificat X.509 et clé privée) est lue dans un fichier PKCS#12
//! (`.p12` / `.pfx`) protégé par mot de passe ; elle signe une empreinte SHA-256 en
//! produisant une structure **CMS détachée** (module [`cms`]), que le module `pdf` place dans
//! le fichier.
//!
//! À ne pas confondre avec `forms::signature`, la signature **visuelle** (une image posée sur
//! la page), qui ne prouve rien cryptographiquement.
//!
//! \[Apprentissage\] La pile RustCrypto découpe le travail en petites crates qui se parlent
//! par des **traits** : `Signer` (sait signer), `Keypair` (connaît sa clé publique),
//! `Digest` (empreinte), `Encode`/`Decode` (DER). Le code ci-dessous choisit le type de clé à
//! l'exécution (RSA ou ECDSA, d'après le fichier) avec une `enum`, puis appelle le même
//! constructeur CMS, générique sur ces traits.

pub mod cms;
mod pdf;

pub use pdf::{SignOptions, SignatureInfo, signatures};

use der::Decode;
use p12_keystore::{KeyStore, KeyStoreEntry, Pkcs12ImportPolicy};
use sha2::{Digest, Sha256};
use x509_cert::Certificate;

use crate::error::{CoreError, CoreResult};

/// Raison d'un verdict « invalide » ou « non vérifiable », sous forme de **code stable** :
/// l'interface le traduit (français, anglais), le CLI affiche [`SigIssue::message`] (C43).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SigIssue {
    /// Structure CMS illisible.
    CmsUnreadable,
    /// Le contenu CMS n'est pas une signature (signedData).
    NotSignedData,
    /// Signature avec contenu encapsulé (non détachée).
    NotDetached,
    /// Plusieurs signataires dans une même signature.
    MultipleSigners,
    /// Aucun signataire.
    NoSigner,
    /// Certificat du signataire absent.
    SignerCertificateMissing,
    /// Algorithme d'empreinte autre que SHA-256.
    DigestAlgorithm,
    /// Signature sans attributs signés.
    NoSignedAttributes,
    /// Empreinte signée absente.
    MessageDigestMissing,
    /// Le contenu a été modifié depuis la signature.
    ContentModified,
    /// Attribut content-type absent, en double ou inattendu.
    ContentTypeInvalid,
    /// Date de signature hors de la période de validité du certificat.
    SigningTimeOutsideValidity,
    /// Attributs signés introuvables dans la structure CMS.
    SignedAttributesUnreadable,
    /// Le certificat joint n'est pas celui que la signature désigne.
    SigningCertificateMismatch,
    /// Clé publique illisible.
    PublicKeyUnreadable,
    /// Clé RSA trop courte (moins de 2048 bits).
    WeakKey,
    /// Clé ECDSA autre que P-256.
    UnsupportedCurve,
    /// Valeur de signature illisible.
    SignatureValueUnreadable,
    /// Signature mathématiquement fausse.
    SignatureMismatch,
    /// Algorithme de signature non pris en charge.
    SignatureAlgorithm,
    /// Format de signature non pris en charge.
    UnsupportedFormat,
    /// /ByteRange absent ou mal formé.
    ByteRangeInvalid,
    /// Valeur /Contents illisible ou mal placée.
    ContentsInvalid,
    /// Signature de droits d'utilisation (UR3), non vérifiée.
    UsageRights,
}

impl SigIssue {
    /// La raison en français (CLI, journaux).
    pub fn message(self) -> &'static str {
        match self {
            SigIssue::CmsUnreadable => "structure CMS illisible",
            SigIssue::NotSignedData => "le contenu CMS n'est pas une signature (signedData)",
            SigIssue::NotDetached => "signature avec contenu encapsulé (non détachée)",
            SigIssue::MultipleSigners => "plusieurs signataires dans une même signature",
            SigIssue::NoSigner => "aucun signataire",
            SigIssue::SignerCertificateMissing => "certificat du signataire absent",
            SigIssue::DigestAlgorithm => "algorithme d'empreinte autre que SHA-256",
            SigIssue::NoSignedAttributes => "signature sans attributs signés",
            SigIssue::MessageDigestMissing => "empreinte signée absente",
            SigIssue::ContentModified => "le contenu a été modifié depuis la signature",
            SigIssue::ContentTypeInvalid => "attribut content-type absent, en double ou inattendu",
            SigIssue::SigningTimeOutsideValidity => {
                "date de signature hors de la période de validité du certificat"
            }
            SigIssue::SignedAttributesUnreadable => {
                "attributs signés introuvables dans la structure CMS"
            }
            SigIssue::SigningCertificateMismatch => {
                "le certificat joint n'est pas celui que la signature désigne"
            }
            SigIssue::PublicKeyUnreadable => "clé publique illisible",
            SigIssue::WeakKey => "clé RSA trop courte (moins de 2048 bits)",
            SigIssue::UnsupportedCurve => "clé ECDSA autre que P-256",
            SigIssue::SignatureValueUnreadable => "valeur de signature illisible",
            SigIssue::SignatureMismatch => "signature mathématiquement fausse",
            SigIssue::SignatureAlgorithm => "algorithme de signature non pris en charge",
            SigIssue::UnsupportedFormat => "format de signature non pris en charge",
            SigIssue::ByteRangeInvalid => "/ByteRange absent ou mal formé",
            SigIssue::ContentsInvalid => "valeur /Contents illisible ou mal placée",
            SigIssue::UsageRights => "signature de droits d'utilisation (UR3), non vérifiée",
        }
    }
}

/// Résultat de la vérification d'une signature.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "issue", rename_all = "snake_case")]
pub enum SignatureStatus {
    /// **Intègre** : l'empreinte correspond, la signature est mathématiquement juste avec la
    /// clé du certificat joint, et la date déclarée tombe dans sa période de validité.
    /// L'**identité n'est pas certifiée** : aucune chaîne de confiance ni révocation n'est
    /// vérifiée (il faudrait le réseau, exclu par le projet). L'interface le dit toujours.
    Intact,
    /// Altérée ou fausse : le contenu signé ne correspond plus, ou la signature est fausse.
    Invalid(SigIssue),
    /// Format ou algorithme que cette version ne sait pas vérifier.
    Unsupported(SigIssue),
}

/// La clé privée d'une identité : RSA (PKCS#1 v1.5) ou ECDSA P-256, avec SHA-256.
pub(crate) enum SigningKey {
    // Une clé RSA pèse plusieurs centaines d'octets : en boîte, l'`enum` reste petite.
    Rsa(Box<rsa::pkcs1v15::SigningKey<Sha256>>),
    Ec(p256::ecdsa::SigningKey),
}

/// Une identité de signature : le certificat du signataire, sa clé privée et la chaîne de
/// certificats du fichier PKCS#12.
pub struct SigningIdentity {
    pub(crate) key: SigningKey,
    pub(crate) certificate: Certificate,
    pub(crate) chain: Vec<Certificate>,
}

impl std::fmt::Debug for SigningIdentity {
    // Jamais la clé privée dans une trace : seulement le nom du signataire.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SigningIdentity")
            .field("signer", &self.signer_name())
            .finish_non_exhaustive()
    }
}

/// Empreinte SHA-256 (algorithme de hachage de toutes nos signatures).
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn unreadable(detail: impl std::fmt::Display) -> CoreError {
    CoreError::InvalidOperation(format!(
        "fichier de certificat illisible (PKCS#12 attendu, .p12 ou .pfx) : {detail}"
    ))
}

impl SigningIdentity {
    /// Lit une identité dans un fichier PKCS#12 protégé par `password`. Un mauvais mot de
    /// passe donne [`CoreError::WrongPassword`] ; un fichier qui n'est pas un PKCS#12 avec
    /// une clé RSA ou ECDSA P-256 et son certificat, une erreur « certificat illisible ».
    pub fn from_pkcs12(bytes: &[u8], password: &str) -> CoreResult<SigningIdentity> {
        let store = match KeyStore::from_pkcs12(bytes, password, Pkcs12ImportPolicy::Strict) {
            Ok(store) => store,
            Err(p12_keystore::error::Error::MacError(_)) => return Err(CoreError::WrongPassword),
            Err(e) => return Err(unreadable(e)),
        };
        let chain = store
            .entries()
            .find_map(|(_, entry)| match entry {
                KeyStoreEntry::PrivateKeyChain(chain) => Some(chain.clone()),
                _ => None,
            })
            .ok_or_else(|| unreadable("aucune clé privée avec son certificat"))?;
        let certs: Vec<Certificate> = chain
            .certs()
            .iter()
            .map(|c| Certificate::from_der(c.as_der()))
            .collect::<Result<_, _>>()
            .map_err(unreadable)?;
        let certificate = certs
            .first()
            .cloned()
            .ok_or_else(|| unreadable("certificat absent"))?;
        let key = signing_key_from_pkcs8(chain.key().as_der())?;
        Ok(SigningIdentity {
            key,
            certificate,
            chain: certs,
        })
    }

    /// Nom du signataire : le nom commun (CN) du certificat, sinon son sujet complet.
    pub fn signer_name(&self) -> String {
        cms::common_name(&self.certificate)
    }
}

/// Clé privée PKCS#8 → clé de signature (RSA d'abord, puis ECDSA P-256).
fn signing_key_from_pkcs8(der: &[u8]) -> CoreResult<SigningKey> {
    use rsa::pkcs8::DecodePrivateKey;
    if let Ok(key) = rsa::RsaPrivateKey::from_pkcs8_der(der) {
        return Ok(SigningKey::Rsa(Box::new(
            rsa::pkcs1v15::SigningKey::<Sha256>::new(key),
        )));
    }
    if let Ok(secret) = p256::SecretKey::from_pkcs8_der(der) {
        return Ok(SigningKey::Ec(p256::ecdsa::SigningKey::from(secret)));
    }
    Err(unreadable(
        "type de clé non pris en charge (RSA ou ECDSA P-256 attendu)",
    ))
}
