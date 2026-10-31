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

/// Ce que la session doit faire d'un document chiffré lu par lopdf.
enum FilterCheck {
    /// Les filtres sont ceux que lopdf sait rechiffrer à l'identique : déchiffrer, travailler,
    /// rechiffrer.
    Standard,
    /// `/StmF` et `/StrF` valent `Identity` : flux et chaînes sont en clair dans le fichier
    /// (seules les pièces jointes, `/EFF`, peuvent être chiffrées). lopdf les « déchiffrerait »
    /// pourtant en RC4 : la session relit alors le fichier sans le déchiffrer et garde son
    /// `/Encrypt` tel quel.
    Passthrough,
}

fn refuse() -> CoreError {
    CoreError::InvalidOperation(
        "document chiffré par filtres (filtre inconnu ou pièces jointes seules mal décrites) : \
         non réécrit par cette version"
            .to_string(),
    )
}

/// Vrai pour le filtre « en clair » : le nom réservé `Identity`, ou l'absence de nom (la norme
/// en fait la valeur par défaut, §7.6.5).
fn is_identity(name: Option<&[u8]>) -> bool {
    // [Apprentissage] `matches!` teste un motif sans écrire un `match` complet ; ici il
    // reconnaît trois formes d'un même sens (absent, vide, `Identity`).
    matches!(name, None | Some(b"") | Some(b"Identity"))
}

/// Choisit le mode de la session pour un document chiffré, ou refuse un chiffrement que lopdf
/// ne rechiffrerait pas à l'identique (relecture du Lot 10) : un filtre `/StmF` ou `/StrF`
/// qui n'est pas une entrée de `/CF` (sans être `Identity` des deux côtés), ou un `/EFF`
/// (pièces jointes) différent de `/StmF`, que lopdf ne sait pas réécrire.
fn check_crypt_filters(doc: &LoDocument, state: &EncryptionState) -> CoreResult<FilterCheck> {
    let encoded = state.encode().map_err(|_| refuse())?;
    let version = encoded.get(b"V").and_then(|v| v.as_i64()).unwrap_or(0);
    if version < 4 {
        return Ok(FilterCheck::Standard);
    }
    let filters: Vec<Vec<u8>> = encoded
        .get(b"CF")
        .and_then(|cf| cf.as_dict())
        .map(|cf| cf.iter().map(|(name, _)| name.clone()).collect())
        .unwrap_or_default();
    let name_of = |key: &[u8]| encoded.get(key).and_then(|n| n.as_name()).ok();
    let stream = name_of(b"StmF");
    if is_identity(stream) && is_identity(name_of(b"StrF")) {
        return Ok(FilterCheck::Passthrough);
    }
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
    Ok(FilterCheck::Standard)
}

/// Copie de `bytes` où chaque référence `/Encrypt N G R` d'une remorque (ou d'un flux de
/// références) est rebaptisée `/Encrypx N G R`, sans changer la longueur : lopdf voit alors un
/// fichier non chiffré et ne touche à aucun octet, et le dictionnaire `/Encrypt` reste un objet
/// ordinaire, comme les flux chiffrés des pièces jointes.
fn hide_encrypt_entry(bytes: &[u8]) -> Vec<u8> {
    const KEY: &[u8] = b"/Encrypt";
    let is_space = |b: u8| matches!(b, b' ' | b'\t' | b'\r' | b'\n' | 0 | 0x0c);
    // Position après « espaces (au moins un) puis chiffres (au moins un) » à partir de `at`.
    let skip_number = |mut at: usize| -> Option<usize> {
        let start = at;
        while bytes.get(at).copied().is_some_and(is_space) {
            at += 1;
        }
        if at == start {
            return None;
        }
        let digits = at;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        (at > digits).then_some(at)
    };
    let mut out = bytes.to_vec();
    let mut from = 0;
    while let Some(found) = bytes[from..].windows(KEY.len()).position(|w| w == KEY) {
        let at = from + found;
        from = at + KEY.len();
        let is_reference = skip_number(from).and_then(skip_number).is_some_and(|end| {
            let mut r = end;
            while bytes.get(r).copied().is_some_and(is_space) {
                r += 1;
            }
            r > end && bytes.get(r) == Some(&b'R')
        });
        if is_reference {
            out[at + KEY.len() - 1] = b'x';
        }
    }
    out
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
        Self::from_loaded(load_limited(bytes, password), bytes, true)
    }

    /// Comme [`LopdfSession::load`], sans réparer la remorque (voir [`load_exact`]).
    pub(crate) fn load_exact(bytes: &[u8], password: Option<&str>) -> CoreResult<Self> {
        Self::from_loaded(load_exact(bytes, password), bytes, false)
    }

    fn from_loaded(
        loaded: lopdf::Result<LoDocument>,
        bytes: &[u8],
        repair: bool,
    ) -> CoreResult<Self> {
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
        if let Some(state) = &state
            && matches!(check_crypt_filters(&doc, state)?, FilterCheck::Passthrough)
        {
            return Self::passthrough(state, bytes, repair);
        }
        Ok(LopdfSession { doc, state })
    }

    /// Session d'un fichier `/StmF /StrF Identity` : relu sans déchiffrement (la première
    /// lecture a déjà authentifié le mot de passe), avec `/Encrypt` remis dans la remorque pour
    /// que l'élagage et la renumérotation le gardent ; `/ID` n'est pas touché. `finish` n'a
    /// rien à rechiffrer : ce qui est chiffré (les pièces jointes) n'a jamais été lu en clair.
    fn passthrough(state: &EncryptionState, bytes: &[u8], repair: bool) -> CoreResult<Self> {
        let hidden = hide_encrypt_entry(bytes);
        let loaded = if repair {
            load_limited(&hidden, None)
        } else {
            load_exact(&hidden, None)
        };
        let mut doc = loaded.map_err(|e| {
            CoreError::InvalidOperation(format!("structure du PDF illisible : {e}"))
        })?;
        let id = state.encrypt_object_id().ok_or_else(refuse)?;
        let is_standard = doc
            .get_dictionary(id)
            .ok()
            .and_then(|d| d.get(b"Filter").and_then(Object::as_name).ok())
            == Some(b"Standard".as_slice());
        if !is_standard {
            return Err(refuse());
        }
        doc.trailer.set("Encrypt", Object::Reference(id));
        Ok(LopdfSession { doc, state: None })
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

/// Version `(majeur, mineur)` annoncée par l'en-tête `%PDF-M.n`, cherché dans les 1024
/// premiers octets (des octets parasites peuvent précéder l'en-tête, comme le tolère la
/// lecture `lopdf`). `None` si aucun en-tête à deux chiffres n'y figure.
///
/// Pourquoi : PDFium ne sait écrire que les versions 1.0 à 1.7 et réécrit un `%PDF-2.0` en
/// `%PDF-1.0` ; sans connaître la version d'origine, chaque édition d'un PDF/A-4 ou d'un
/// PDF/UA-2 romprait sa conformité (audit E2).
pub(crate) fn header_version(bytes: &[u8]) -> Option<(u8, u8)> {
    let head = &bytes[..bytes.len().min(1024)];
    let start = head.windows(5).position(|w| w == b"%PDF-")? + 5;
    let digits = head.get(start..start + 3)?;
    match digits {
        [major @ b'0'..=b'9', b'.', minor @ b'0'..=b'9'] => Some((major - b'0', minor - b'0')),
        _ => None,
    }
}

/// Copie de `bytes` dont l'en-tête annonce au moins `min_version`, sans changer la longueur
/// (les trois octets `M.n` sont réécrits en place : tous les décalages restent exacts).
/// Les octets sont rendus tels quels si l'en-tête est déjà assez récent ou introuvable.
pub(crate) fn raise_header_version(bytes: &[u8], min_version: Option<(u8, u8)>) -> Vec<u8> {
    let mut out = bytes.to_vec();
    if let (Some(min), Some(current)) = (min_version, header_version(bytes))
        // [Apprentissage] Les tuples se comparent champ par champ : (2, 0) > (1, 7).
        && min > current
        && let Some(pos) = out[..out.len().min(1024)]
            .windows(5)
            .position(|w| w == b"%PDF-")
    {
        out[pos + 5] = b'0' + min.0;
        out[pos + 7] = b'0' + min.1;
    }
    out
}

#[cfg(test)]
mod header_tests {
    use super::*;

    #[test]
    fn header_version_reads_major_minor_after_a_prefix() {
        assert_eq!(header_version(b"%PDF-2.0\n%\xE2\xE3"), Some((2, 0)));
        assert_eq!(header_version(b"junk\n%PDF-1.4\n"), Some((1, 4)));
        assert_eq!(header_version(b"nope"), None);
    }

    #[test]
    fn raise_header_version_only_raises() {
        assert_eq!(
            raise_header_version(b"%PDF-1.4\nx", Some((2, 0))),
            b"%PDF-2.0\nx"
        );
        assert_eq!(
            raise_header_version(b"%PDF-1.7\nx", Some((1, 4))),
            b"%PDF-1.7\nx"
        );
        assert_eq!(raise_header_version(b"%PDF-1.7\nx", None), b"%PDF-1.7\nx");
    }
}
