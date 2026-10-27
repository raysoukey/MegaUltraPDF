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

/// Résultat de la vérification d'une signature.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum SignatureStatus {
    /// **Intègre** : l'empreinte correspond, la signature est mathématiquement juste avec la
    /// clé du certificat joint, et la date déclarée tombe dans sa période de validité.
    /// L'**identité n'est pas certifiée** : aucune chaîne de confiance ni révocation n'est
    /// vérifiée (il faudrait le réseau, exclu par le projet). L'interface le dit toujours.
    Intact,
    /// Altérée ou fausse : le contenu signé ne correspond plus, ou la signature est fausse.
    Invalid(String),
    /// Format ou algorithme que cette version ne sait pas vérifier.
    Unsupported(String),
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
