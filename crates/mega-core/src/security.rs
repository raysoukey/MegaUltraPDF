// SPDX-License-Identifier: GPL-3.0-or-later
//! Protection d'un document par mot de passe (AES-256, ISO 32000-2 §7.6.4, révision 6) et
//! retrait de cette protection, par `lopdf` ; lecture de l'état de sécurité par PDFium.
//!
//! \[Apprentissage\] **D'où vient l'aléa ?** Une clé de chiffrement doit être imprévisible :
//! on la tire de `rand::rng()`, un générateur alimenté par le système (`getrandom`). Un
//! mot de passe, lui, n'est pas une clé : la norme dérive la clé par des hachages salés
//! (algorithme 8/9), ce que fait `lopdf`. Les deux mots de passe sont des **chaînes** que
//! l'on convertit en octets UTF-8 après normalisation SASLprep (révision 6) ; voir
//! `check_password` pour les mots de passe acceptés.

use std::collections::BTreeMap;
use std::sync::Arc;

use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
use lopdf::{EncryptionState, EncryptionVersion};
use pdfium_render::prelude::PdfSecurityHandlerRevision;
use rand::RngExt as _;
use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::engine;
use crate::error::{CoreError, CoreResult};

/// Longueur maximale d'un mot de passe, en octets (révision 6 : 127).
pub const MAX_PASSWORD_BYTES: usize = 127;

/// Ce que le lecteur a le droit de faire sans le mot de passe propriétaire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permissions {
    /// Imprimer.
    pub print: bool,
    /// Copier le texte et les images.
    pub copy: bool,
    /// Modifier le contenu (pages, texte).
    pub modify: bool,
    /// Annoter et remplir les formulaires.
    pub annotate: bool,
}

/// Demande de protection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Protection {
    /// Mot de passe d'ouverture (vide : le document s'ouvre sans mot de passe).
    pub user_password: String,
    /// Mot de passe propriétaire (vide : égal au mot de passe d'ouverture).
    pub owner_password: String,
    /// Permissions accordées sans le mot de passe propriétaire.
    pub permissions: Permissions,
}

/// État de sécurité d'un document ouvert.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityInfo {
    /// Le fichier est chiffré.
    pub encrypted: bool,
    /// Il a fallu un mot de passe pour l'ouvrir : le fichier a donc un mot de passe
    /// **utilisateur** non vide (PDFium ouvre sans rien demander un fichier protégé par le
    /// seul mot de passe propriétaire). Qu'il s'agisse du mot de passe utilisateur ou
    /// propriétaire, la personne qui l'a saisi peut retirer la protection.
    pub user_password_required: bool,
    /// Permissions telles que PDFium les lit.
    pub permissions: Permissions,
}

fn invalid(msg: &str) -> CoreError {
    CoreError::InvalidOperation(msg.to_string())
}

/// Vérifie un mot de passe : longueur en octets, et invariance par **SASLprep** (RFC 4013).
///
/// La révision 6 normalise le mot de passe par SASLprep avant d'en dériver la clé ; `lopdf`
/// le fait en chiffrant, PDFium ne le fait pas en ouvrant. Un mot de passe que SASLprep
/// modifie (espace insécable, « ² », ligature « ﬁ ») serait donc chiffré sous une forme et
/// essayé sous une autre : le fichier ne se rouvrirait plus. Tous les autres sont acceptés,
/// lettres de n'importe quelle écriture comprises (audit du Lot 13, C10 ; la limite était
/// U+00FF). Le message ne cite jamais le caractère fautif : il pourrait finir dans le
/// journal de diagnostic.
fn check_password(what: &str, password: &str) -> CoreResult<()> {
    if password.len() > MAX_PASSWORD_BYTES {
        return Err(CoreError::InvalidOperation(format!(
            "le mot de passe {what} dépasse {MAX_PASSWORD_BYTES} octets"
        )));
    }
    let unchanged = stringprep::saslprep(password).is_ok_and(|prepared| prepared == password);
    if !unchanged {
        return Err(CoreError::InvalidOperation(format!(
            "le mot de passe {what} contient un caractère que la norme réécrit ou interdit \
             (caractère de contrôle, espace insécable, exposant, ligature, mélange de sens d'écriture…)"
        )));
    }
    Ok(())
}

/// Nos permissions → les drapeaux de lopdf (§7.6.4.2, tableau 22). Le remplissage de
/// formulaire et l'accessibilité suivent respectivement « annoter » et « copier » ;
/// l'assemblage suit « modifier ».
fn to_lopdf(p: Permissions) -> lopdf::Permissions {
    use lopdf::Permissions as P;
    // Bit 10 (extraction pour l'accessibilité) : déprécié en PDF 2.0, il doit rester à 1 ;
    // à 0, Acrobat refuse le contenu aux lecteurs d'écran.
    let mut flags = P::COPYABLE_FOR_ACCESSIBILITY;
    if p.print {
        flags |= P::PRINTABLE | P::PRINTABLE_IN_HIGH_QUALITY;
    }
    if p.copy {
        flags |= P::COPYABLE;
    }
    if p.modify {
        flags |= P::MODIFIABLE | P::ASSEMBLABLE;
    }
    if p.annotate {
        flags |= P::ANNOTABLE | P::FILLABLE;
    }
    flags
}

/// lopdf 0.46 n'écrit ni `/Length 256` dans `/Encrypt` ni `/Length 32 /AuthEvent /DocOpen`
/// dans le filtre `/StdCF` : facultatifs pour la révision 6, mais `qpdf --check` les réclame
/// et certains validateurs aussi. On les ajoute (valeurs de la norme, tableau 20 et 25).
fn complete_encrypt_dictionary(doc: &mut lopdf::Document) {
    let Ok(id) = doc
        .trailer
        .get(b"Encrypt")
        .and_then(lopdf::Object::as_reference)
    else {
        return;
    };
    let Ok(enc) = doc.get_dictionary_mut(id) else {
        return;
    };
    enc.set("Length", 256);
    if let Ok(lopdf::Object::Dictionary(filters)) = enc.get_mut(b"CF")
        && let Ok(lopdf::Object::Dictionary(std_cf)) = filters.get_mut(b"StdCF")
    {
        std_cf.set("Length", 32);
        std_cf.set("AuthEvent", lopdf::Object::Name(b"DocOpen".to_vec()));
    }
}

impl Document {
    /// État de sécurité : chiffré ou non, mot de passe exigé à l'ouverture, permissions.
    ///
    /// pdfium-render 0.9.4 ne connaît que les révisions 2 à 4 du gestionnaire de sécurité :
    /// pour une révision 5 ou 6 (AES-256, celle que nous écrivons) ses accesseurs échouent.
    /// On lit alors les drapeaux `/P` du dictionnaire `/Encrypt` nous-mêmes (§7.6.4.2,
    /// tableau 22) : il est lisible sans mot de passe.
    pub fn security(&self) -> CoreResult<SecurityInfo> {
        let _guard = engine::guard();
        let perms = self.inner().permissions();
        let (encrypted, permissions) = match perms.security_handler_revision() {
            Ok(PdfSecurityHandlerRevision::Unprotected) => (
                false,
                Permissions {
                    print: true,
                    copy: true,
                    modify: true,
                    annotate: true,
                },
            ),
            Ok(_) => (
                true,
                Permissions {
                    print: perms.can_print_high_quality()? || perms.can_print_only_low_quality()?,
                    copy: perms.can_extract_text_and_graphics()?,
                    modify: perms.can_modify_document_content()?,
                    annotate: perms.can_add_or_modify_text_annotations()?,
                },
            ),
            Err(_) => (true, self.permissions_from_encrypt_dict_unlocked()),
        };
        Ok(SecurityInfo {
            encrypted,
            // Le mot de passe mémorisé n'a de sens que si le document est encore chiffré
            // (après « annuler » une protection, il peut rester en mémoire).
            user_password_required: encrypted && self.password().is_some(),
            permissions,
        })
    }

    /// Drapeaux `/P` lus dans `/Encrypt` (jeton déjà pris) ; tout refusé si illisibles.
    fn permissions_from_encrypt_dict_unlocked(&self) -> Permissions {
        let denied = Permissions {
            print: false,
            copy: false,
            modify: false,
            annotate: false,
        };
        // Rien de changé : `/Encrypt` se lit dans les octets du fichier, sans réécriture (C15).
        let saved;
        let bytes: &[u8] = if self.changed {
            let Ok(b) = self.inner().save_to_bytes() else {
                return denied;
            };
            saved = b;
            &saved
        } else {
            self.file_bytes()
        };
        let Ok(doc) = crate::crypt::load_limited(bytes, None) else {
            return denied;
        };
        // Mot de passe utilisateur vide : lopdf a déjà déchiffré et retiré `/Encrypt` du
        // trailer ; les permissions sont alors dans l'état de chiffrement qu'il a gardé.
        if let Some(state) = doc.encryption_state.as_ref() {
            use lopdf::Permissions as P;
            let flags = state.permissions();
            return Permissions {
                print: flags.contains(P::PRINTABLE),
                modify: flags.contains(P::MODIFIABLE),
                copy: flags.contains(P::COPYABLE),
                annotate: flags.contains(P::ANNOTABLE),
            };
        }
        let p = doc
            .trailer
            .get(b"Encrypt")
            .ok()
            .and_then(|e| doc.dereference(e).ok())
            .and_then(|(_, e)| e.as_dict().ok().cloned())
            .and_then(|d| d.get(b"P").ok().and_then(|p| p.as_i64().ok()));
        let Some(p) = p else {
            return denied;
        };
        let bits = p as u32;
        Permissions {
            print: bits & (1 << 2) != 0,
            modify: bits & (1 << 3) != 0,
            copy: bits & (1 << 4) != 0,
            annotate: bits & (1 << 5) != 0,
        }
    }

    /// Protège le document (AES-256) : il est rechargé aussitôt, avec son mot de passe, et
    /// reste utilisable ; les passes `lopdf` ultérieures (métadonnées, filigrane…) le
    /// refuseront : protéger est la **dernière** étape. Refusé si déjà chiffré.
    pub fn protect(&mut self, p: &Protection) -> CoreResult<()> {
        self.mark_changed();
        if p.user_password.is_empty() && p.owner_password.is_empty() {
            return Err(invalid("indiquez au moins un mot de passe"));
        }
        check_password("d'ouverture", &p.user_password)?;
        check_password("propriétaire", &p.owner_password)?;
        if self.security()?.encrypted {
            return Err(invalid(
                "le document est déjà protégé : retirez d'abord sa protection",
            ));
        }
        let owner = if p.owner_password.is_empty() {
            p.user_password.as_str()
        } else {
            p.owner_password.as_str()
        };
        // La passe d'enregistrement (`/InkList`, clé privée de PDFium, orphelins) doit avoir
        // lieu **avant** le chiffrement : ensuite, elle s'abstient sur un fichier chiffré.
        let bytes =
            crate::cleanup::before_save(&self.to_bytes()?, self.password(), self.file_version())?
                .into_checked(self.needs_cleanup, "protection refusée")?;
        let mut doc = crate::crypt::load_limited(&bytes, None).map_err(|e| {
            CoreError::InvalidOperation(format!("structure du PDF illisible : {e}"))
        })?;
        if doc.is_encrypted() || doc.was_encrypted() {
            return Err(invalid("le document est déjà protégé"));
        }
        let mut key = [0u8; 32];
        rand::rng().fill(&mut key[..]);
        let mut filters: BTreeMap<Vec<u8>, Arc<dyn CryptFilter>> = BTreeMap::new();
        filters.insert(b"StdCF".to_vec(), Arc::new(Aes256CryptFilter));
        let state = EncryptionState::try_from(EncryptionVersion::V5 {
            encrypt_metadata: true,
            crypt_filters: filters,
            file_encryption_key: &key,
            stream_filter: b"StdCF".to_vec(),
            string_filter: b"StdCF".to_vec(),
            owner_password: owner,
            user_password: &p.user_password,
            permissions: to_lopdf(p.permissions),
        })
        .map_err(|e| CoreError::InvalidOperation(format!("préparation du chiffrement : {e}")))?;
        doc.encrypt(&state)
            .map_err(|e| CoreError::InvalidOperation(format!("chiffrement : {e}")))?;
        complete_encrypt_dictionary(&mut doc);
        declare_aes256_extension(&mut doc);
        let mut out = Vec::new();
        doc.save_to(&mut out)?;
        let previous = self.password().map(str::to_string);
        self.set_password((!p.user_password.is_empty()).then(|| p.user_password.clone()));
        if let Err(e) = self.reload_from_bytes(out) {
            self.set_password(previous);
            return Err(e);
        }
        Ok(())
    }

    /// Retire la protection d'un document ouvert **avec** son mot de passe (utilisateur ou
    /// propriétaire). Un document chiffré ouvert sans mot de passe (protection propriétaire
    /// seule) ne peut pas prouver le droit de le faire : `WrongPassword`.
    pub fn unprotect(&mut self) -> CoreResult<()> {
        self.mark_changed();
        self.unprotect_with(None)
    }

    /// Comme [`unprotect`](Self::unprotect), avec un mot de passe saisi (prioritaire sur celui
    /// de l'ouverture) : c'est ainsi qu'on déprotège un fichier à mot de passe propriétaire
    /// seul, qui s'ouvre sans rien demander. Un mot de passe faux donne `WrongPassword`.
    pub fn unprotect_with(&mut self, typed: Option<&str>) -> CoreResult<()> {
        self.mark_changed();
        if !self.security()?.encrypted {
            return Err(invalid("le document n'est pas protégé"));
        }
        let password = match typed.filter(|p| !p.is_empty()) {
            Some(p) => p.to_string(),
            None => match self.password() {
                Some(p) => p.to_string(),
                None => return Err(CoreError::WrongPassword),
            },
        };
        let bytes = self.to_bytes()?;
        // Le mot de passe doit être **vérifié** : lopdf ouvre un fichier à mot de passe
        // utilisateur vide quel que soit celui qu'on lui donne. PDFium, lui, n'ouvre qu'avec
        // un mot de passe exact (utilisateur ou propriétaire) : c'est notre preuve.
        {
            let guard = engine::guard();
            if engine::load_bytes(&guard, bytes.clone(), Some(&password)).is_err() {
                return Err(CoreError::WrongPassword);
            }
        }
        let mut doc = match crate::crypt::load_limited(&bytes, Some(&password)) {
            Ok(doc) => doc,
            Err(_) => return Err(CoreError::WrongPassword),
        };
        if doc.is_encrypted() {
            // lopdf n'a pas pu s'authentifier à la lecture : dernier essai explicite.
            doc.decrypt(&password)
                .map_err(|_| CoreError::WrongPassword)?;
        }
        let mut out = Vec::new();
        doc.save_to(&mut out)?;
        self.set_password(None);
        if let Err(e) = self.reload_from_bytes(out) {
            self.set_password(Some(password));
            return Err(e);
        }
        Ok(())
    }
}

/// Déclare que le fichier utilise l'AES-256 (révision 6), absent de PDF 1.7 : l'en-tête monte
/// à 1.7 au moins (une version plus haute reste) et le catalogue reçoit
/// `/Extensions << /ADBE << /BaseVersion /1.7 /ExtensionLevel 8 >> >>` (Adobe Supplement to
/// ISO 32000-1 ; ISO 32000-2, 7.6.4). Sans cela, un lecteur strict ne sait pas lire le fichier.
fn declare_aes256_extension(doc: &mut lopdf::Document) {
    use lopdf::dictionary;
    if crate::crypt::parse_version(doc.version.as_bytes()).is_none_or(|v| v < (1, 7)) {
        doc.version = "1.7".to_string();
    }
    let Some(root) = doc
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|r| r.as_reference().ok())
    else {
        return;
    };
    let Ok(catalog) = doc.get_dictionary(root) else {
        return;
    };
    // Une table `/Extensions` peut être un objet indirect : on la complète là où elle vit.
    let indirect = catalog
        .get(b"Extensions")
        .and_then(lopdf::Object::as_reference)
        .ok()
        .filter(|id| doc.get_dictionary(*id).is_ok());
    let existing = match indirect {
        Some(id) => doc.get_dictionary(id).ok(),
        None => catalog
            .get(b"Extensions")
            .and_then(lopdf::Object::as_dict)
            .ok(),
    };
    // Une extension d'Adobe de niveau 8 ou plus (plus récente, qui inclut la nôtre) reste.
    if existing.is_some_and(|ext| adobe_extension_level(doc, ext).is_some_and(|l| l >= 8)) {
        return;
    }
    let adbe = dictionary! {
        "BaseVersion" => lopdf::Object::Name(b"1.7".to_vec()),
        "ExtensionLevel" => 8,
    };
    // Une table `/Extensions` existante est complétée, pas remplacée (autres développeurs).
    if let Some(id) = indirect {
        if let Ok(extensions) = doc.get_dictionary_mut(id) {
            extensions.set("ADBE", adbe);
        }
        return;
    }
    let Ok(catalog) = doc.get_dictionary_mut(root) else {
        return;
    };
    if let Ok(existing) = catalog.get_mut(b"Extensions").and_then(|e| e.as_dict_mut()) {
        existing.set("ADBE", adbe);
    } else {
        catalog.set("Extensions", dictionary! { "ADBE" => adbe });
    }
}

/// Plus haut `/ExtensionLevel` de l'entrée `/ADBE` d'une table `/Extensions` : un dictionnaire,
/// ou (PDF 2.0, §7.12.3) un tableau de dictionnaires ; références suivies.
fn adobe_extension_level(doc: &lopdf::Document, extensions: &lopdf::Dictionary) -> Option<i64> {
    let level = |entry: &lopdf::Object| -> Option<i64> {
        let (_, entry) = doc.dereference(entry).ok()?;
        let raw = entry.as_dict().ok()?.get(b"ExtensionLevel").ok()?;
        doc.dereference(raw).ok()?.1.as_i64().ok()
    };
    let (_, adbe) = doc.dereference(extensions.get(b"ADBE").ok()?).ok()?;
    match adbe {
        lopdf::Object::Array(items) => items.iter().filter_map(level).max(),
        other => level(other),
    }
}

#[cfg(test)]
mod extension_tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use lopdf::{Object, dictionary};

    /// Un catalogue dont `/Extensions` vaut `extensions` (objet 3 si `indirect`).
    fn doc_with(extensions: lopdf::Dictionary, indirect: bool) -> lopdf::Document {
        let mut doc = lopdf::Document::with_version("1.7");
        let value = if indirect {
            Object::Reference(doc.add_object(extensions))
        } else {
            Object::Dictionary(extensions)
        };
        let root = doc.add_object(dictionary! { "Type" => "Catalog", "Extensions" => value });
        doc.trailer.set("Root", root);
        doc
    }

    /// Le dictionnaire `/Extensions` du catalogue, déréférencé.
    fn extensions(doc: &lopdf::Document) -> lopdf::Dictionary {
        let ext = doc.catalog().unwrap().get(b"Extensions").unwrap();
        doc.dereference(ext).unwrap().1.as_dict().unwrap().clone()
    }

    fn level(ext: &lopdf::Dictionary) -> i64 {
        ext.get(b"ADBE")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"ExtensionLevel")
            .unwrap()
            .as_i64()
            .unwrap()
    }

    /// Revue de la partie C : un `/Extensions` indirect est complété là où il vit, ses autres
    /// entrées gardées.
    #[test]
    fn an_indirect_extensions_dictionary_is_completed() {
        let other =
            dictionary! { "BaseVersion" => Object::Name(b"1.7".to_vec()), "ExtensionLevel" => 1 };
        let mut doc = doc_with(dictionary! { "XYZ" => other }, true);
        declare_aes256_extension(&mut doc);
        assert!(
            doc.catalog()
                .unwrap()
                .get(b"Extensions")
                .unwrap()
                .as_reference()
                .is_ok(),
            "la référence reste"
        );
        let ext = extensions(&doc);
        assert!(ext.has(b"XYZ"), "{ext:?}");
        assert_eq!(level(&ext), 8);
    }

    /// Un `/ADBE` de niveau 8 ou plus (une extension plus récente d'Adobe) est gardé ; un
    /// niveau inférieur est remplacé.
    #[test]
    fn a_higher_adobe_extension_level_is_kept() {
        let adbe = |n: i64| {
            dictionary! { "ADBE" => dictionary! {
                "BaseVersion" => Object::Name(b"1.7".to_vec()), "ExtensionLevel" => n
            } }
        };
        let mut high = doc_with(adbe(11), false);
        declare_aes256_extension(&mut high);
        assert_eq!(level(&extensions(&high)), 11);
        let mut low = doc_with(adbe(3), true);
        declare_aes256_extension(&mut low);
        assert_eq!(level(&extensions(&low)), 8);
    }
}
