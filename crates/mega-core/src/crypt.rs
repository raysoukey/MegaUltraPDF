// SPDX-License-Identifier: GPL-3.0-or-later
//! Réécrire un PDF **chiffré** avec `lopdf` sans perdre sa protection (Lot 10, ADR-0014).
//!
//! Nos passes `lopdf` (nettoyage d'enregistrement, filigrane, caviardage des images, couche
//! OCR) travaillent sur des objets **déchiffrés**. Une [`LopdfSession`] charge le fichier avec
//! son mot de passe, garde l'état de chiffrement (clé de fichier, valeurs `/O` `/U` `/P`,
//! filtres), laisse la passe travailler, puis **rechiffre avec ce même état** : le fichier
//! s'ouvre avec le même mot de passe et garde les mêmes permissions ; `/ID` (qui entre dans la
//! clé en RC4 et AES-128) n'est pas touché.
//!
//! Un document protégé par un mot de passe **propriétaire** seul s'ouvre sans mot de passe :
//! lopdf le déchiffre alors en silence (piège du Lot 6, ADR-0011) ; la session le rechiffre
//! tout autant.
//!
//! \[Apprentissage\] **`Option::take`** : `doc.encryption_state.take()` sort l'état du document
//! (le champ devient `None`) et nous le confie ; c'est nous qui le rendrons au chiffrement
//! dans [`LopdfSession::finish`]. Un seul propriétaire à la fois, jamais de copie oubliée.

use lopdf::encryption::EncryptionState;
use lopdf::{Document as LoDocument, LoadOptions};

use crate::error::{CoreError, CoreResult};

/// Refuse un chiffrement que lopdf ne rechiffrerait pas à l'identique (relecture du Lot 10) :
/// un filtre `/StmF` ou `/StrF` qui n'est pas une entrée de `/CF` (le nom réservé `Identity`,
/// « en clair », par exemple : lopdf le traiterait en RC4 et brouillerait le fichier), ou un
/// `/EFF` (pièces jointes) différent de `/StmF`, que lopdf ne sait pas réécrire.
fn check_crypt_filters(doc: &LoDocument, state: &EncryptionState) -> CoreResult<()> {
    let refuse = || {
        CoreError::InvalidOperation(
            "document chiffré par filtres (pièces jointes seules, filtre « Identity ») : \
             non réécrit par cette version"
                .to_string(),
        )
    };
    let encoded = state.encode().map_err(|_| refuse())?;
    let version = encoded.get(b"V").and_then(|v| v.as_i64()).unwrap_or(0);
    if version < 4 {
        return Ok(());
    }
    let filters: Vec<Vec<u8>> = encoded
        .get(b"CF")
        .and_then(|cf| cf.as_dict())
        .map(|cf| cf.iter().map(|(name, _)| name.clone()).collect())
        .unwrap_or_default();
    let name_of = |key: &[u8]| encoded.get(key).and_then(|n| n.as_name()).ok();
    let stream = name_of(b"StmF");
    for key in [&b"StmF"[..], b"StrF"] {
        match name_of(key) {
            Some(name) if filters.iter().any(|f| f.as_slice() == name) => {}
            _ => return Err(refuse()),
        }
    }
    // `/EFF` n'est pas gardé par lopdf : on le lit dans le dictionnaire `/Encrypt` d'origine,
    // resté parmi les objets (il n'est jamais déchiffré).
    let eff = doc.objects.values().find_map(|object| {
        let dict = object.as_dict().ok()?;
        let standard = dict.get(b"Filter").and_then(|f| f.as_name()).ok() == Some(b"Standard");
        (standard && dict.has(b"O") && dict.has(b"U"))
            .then(|| {
                dict.get(b"EFF")
                    .and_then(|e| e.as_name())
                    .ok()
                    .map(<[u8]>::to_vec)
            })
            .flatten()
    });
    if let Some(eff) = eff
        && Some(eff.as_slice()) != stream
    {
        return Err(refuse());
    }
    Ok(())
}

/// Un document `lopdf` déchiffré le temps d'une passe, à rechiffrer à l'identique.
pub(crate) struct LopdfSession {
    /// Le document, objets en clair.
    pub(crate) doc: LoDocument,
    /// L'état de chiffrement d'origine (`None` : document non chiffré).
    state: Option<EncryptionState>,
}

impl LopdfSession {
    /// Charge `bytes` (déchiffré avec `password`, ou le mot de passe vide s'il n'y en a pas).
    /// Refuse un document resté chiffré : mot de passe faux, ou chiffrement que lopdf ne sait
    /// pas lire (filtre inconnu).
    pub(crate) fn load(bytes: &[u8], password: Option<&str>) -> CoreResult<Self> {
        let options = match password {
            Some(p) => LoadOptions::with_password(p),
            None => LoadOptions::default(),
        };
        let mut doc = LoDocument::load_mem_with_options(bytes, options).map_err(|e| {
            CoreError::InvalidOperation(format!("structure du PDF illisible : {e}"))
        })?;
        if doc.is_encrypted() {
            return Err(CoreError::InvalidOperation(
                "document chiffré que cette version ne sait pas réécrire (mot de passe ou \
                 chiffrement non pris en charge)"
                    .to_string(),
            ));
        }
        let state = doc.encryption_state.take();
        if let Some(state) = &state {
            check_crypt_filters(&doc, state)?;
        }
        Ok(LopdfSession { doc, state })
    }

    /// Rechiffre (si besoin) avec l'état d'origine et sérialise le document.
    pub(crate) fn finish(mut self) -> CoreResult<Vec<u8>> {
        if let Some(state) = &self.state {
            self.doc.encrypt(state).map_err(|e| {
                CoreError::InvalidOperation(format!("rechiffrement du document : {e}"))
            })?;
        }
        let mut out = Vec::new();
        self.doc.save_to(&mut out)?;
        Ok(out)
    }
}
