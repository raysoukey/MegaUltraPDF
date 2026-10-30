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
use lopdf::{Document as LoDocument, LoadOptions, Object, ObjectId, Stream};

use crate::error::{CoreError, CoreResult};

/// Taille décompressée maximale d'un flux lu par `lopdf` (256 Mio), et du contenu complet
/// d'une page (audit du Lot 13, R1).
///
/// Un flux compressé peut tenir en quelques centaines de Ko et se décompresser en
/// gigaoctets (« bombe de décompression ») : sans limite, la simple passe d'enregistrement
/// épuiserait la mémoire. Aucun document réel n'approche ce plafond pour un seul flux.
pub(crate) const MAX_DECOMPRESSED: usize = 256 * 1024 * 1024;

/// Charge `bytes` avec `lopdf` (déchiffré avec `password` s'il est donné), sans qu'aucun flux
/// d'objets ou de références ne se décompresse au-delà de [`MAX_DECOMPRESSED`]. **Toute**
/// lecture `lopdf` du crate passe par ici, jamais par `Document::load_mem` directement.
///
/// Une remorque sans `/Root` (fichier abîmé) est réparée comme le fait PDFium : le catalogue
/// est retrouvé parmi les objets (audit du Lot 13, R8). Sans cela, PDFium affichait le
/// document mais nos passes `lopdf` n'y voyaient ni formulaire ni signature.
pub(crate) fn load_limited(bytes: &[u8], password: Option<&str>) -> lopdf::Result<LoDocument> {
    let mut doc = load_exact(bytes, password)?;
    if doc.trailer.get(b"Root").is_err()
        && let Some(catalog) = find_catalog(&doc)
    {
        doc.trailer.set("Root", Object::Reference(catalog));
    }
    Ok(doc)
}

/// Comme [`load_limited`], **sans** réparer la remorque : pour les contrôles qui jugent une
/// réécriture (`cleanup::rewrite_is_sane`). Une sortie sans `/Root` doit y être refusée, pas
/// rattrapée en mémoire (relecture de la PR A du Lot 13).
pub(crate) fn load_exact(bytes: &[u8], password: Option<&str>) -> lopdf::Result<LoDocument> {
    let options = LoadOptions {
        password: password.map(str::to_string),
        max_decompressed_size: Some(MAX_DECOMPRESSED),
        ..LoadOptions::default()
    };
    LoDocument::load_mem_with_options(bytes, options)
}

/// Le catalogue d'un fichier dont la remorque l'a perdu : parmi les objets `/Type /Catalog`,
/// ceux dont l'arbre `/Pages` se résout en dictionnaire passent d'abord (un catalogue orphelin
/// laissé par une fusion maladroite n'en a souvent pas), puis le plus récent (numéro d'objet
/// le plus grand).
fn find_catalog(doc: &LoDocument) -> Option<ObjectId> {
    let has_pages = |dict: &lopdf::Dictionary| {
        dict.get(b"Pages")
            .ok()
            .and_then(|p| doc.dereference(p).ok())
            .is_some_and(|(_, p)| p.as_dict().is_ok())
    };
    doc.objects
        .iter()
        .filter_map(|(id, object)| {
            let dict = object.as_dict().ok()?;
            let is_catalog = dict.get(b"Type").ok().and_then(|t| t.as_name().ok())
                == Some(b"Catalog".as_slice());
            is_catalog.then(|| (has_pages(dict), *id))
        })
        .max()
        .map(|(_, id)| id)
}

/// Contenu décodé de la page `page_id` (flux concaténés), plafonné à [`MAX_DECOMPRESSED`] :
/// remplace `get_page_content`, qui décompresse sans limite.
pub(crate) fn page_content(doc: &LoDocument, page_id: ObjectId) -> lopdf::Result<Vec<u8>> {
    doc.get_page_content_with_limit(page_id, MAX_DECOMPRESSED)
}

/// Octets décodés d'un flux, ou ses octets bruts s'il n'est pas compressé (ou si son filtre
/// est inconnu) ; `None` s'il dépasserait [`MAX_DECOMPRESSED`] une fois décompressé.
pub(crate) fn stream_bytes(stream: &Stream) -> Option<Vec<u8>> {
    match stream.decompressed_content_with_limit(MAX_DECOMPRESSED) {
        Ok(data) => Some(data),
        Err(lopdf::Error::Decompress(lopdf::DecompressError::MemoryLimitExceeded { .. })) => None,
        Err(_) => Some(stream.content.clone()),
    }
}

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
        Self::from_loaded(load_limited(bytes, password))
    }

    /// Comme [`LopdfSession::load`], sans réparer la remorque (voir [`load_exact`]).
    pub(crate) fn load_exact(bytes: &[u8], password: Option<&str>) -> CoreResult<Self> {
        Self::from_loaded(load_exact(bytes, password))
    }

    fn from_loaded(loaded: lopdf::Result<LoDocument>) -> CoreResult<Self> {
        let mut doc = loaded.map_err(|e| {
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
