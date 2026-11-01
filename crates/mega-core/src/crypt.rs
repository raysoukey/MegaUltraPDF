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
    stream_bytes_within(stream, MAX_DECOMPRESSED)
}

/// Comme [`stream_bytes`], avec un plafond `limit` choisi par l'appelant (au plus
/// [`MAX_DECOMPRESSED`]) : une lecture qui décode beaucoup de flux tient ainsi un budget
/// global (repli de la couche de texte). `None` si le flux dépasserait ce plafond.
pub(crate) fn stream_bytes_within(stream: &Stream, limit: usize) -> Option<Vec<u8>> {
    let limit = limit.min(MAX_DECOMPRESSED);
    match stream.decompressed_content_with_limit(limit) {
        Ok(data) => (data.len() <= limit).then_some(data),
        Err(lopdf::Error::Decompress(lopdf::DecompressError::MemoryLimitExceeded { .. })) => None,
        Err(_) => (stream.content.len() <= limit).then(|| stream.content.clone()),
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

/// Copie de `bytes` où la référence `/Encrypt N G R` de chaque remorque (le dictionnaire qui
/// suit un mot-clé `trailer`) est rebaptisée `/Encrypx N G R`, sans changer la longueur : lopdf
/// voit alors un fichier non chiffré et ne touche à aucun octet, et le dictionnaire `/Encrypt`
/// reste un objet ordinaire, comme les flux chiffrés des pièces jointes.
///
/// Rien d'autre n'est modifié : dans un fichier `Identity`, chaînes et flux non compressés sont
/// en clair et pourraient citer le même texte. `None` si aucune remorque classique ne porte la
/// référence (fichier à flux de références : elle y est dans un flux, non réécrit ici).
fn hide_encrypt_entry(bytes: &[u8]) -> Option<Vec<u8>> {
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
    let mut renamed = false;
    let mut from = 0;
    while let Some(found) = bytes[from..].windows(7).position(|w| w == b"trailer") {
        let mut at = from + found + 7;
        from = at;
        while bytes.get(at).copied().is_some_and(is_space) {
            at += 1;
        }
        if !bytes[at..].starts_with(b"<<") {
            continue;
        }
        // Fin du dictionnaire de remorque : `<<` et `>>` équilibrés (un `<` seul ouvre une
        // chaîne hexadécimale, il n'est pas compté).
        let start = at;
        let mut depth = 0usize;
        while at < bytes.len() {
            if bytes[at..].starts_with(b"<<") {
                depth += 1;
                at += 2;
            } else if bytes[at..].starts_with(b">>") {
                depth = depth.saturating_sub(1);
                at += 2;
                if depth == 0 {
                    break;
                }
            } else {
                at += 1;
            }
        }
        let end = at;
        from = end;
        let mut scan = start;
        while let Some(found) = bytes[scan..end].windows(KEY.len()).position(|w| w == KEY) {
            let key_at = scan + found;
            scan = key_at + KEY.len();
            let is_reference = skip_number(scan)
                .and_then(skip_number)
                .is_some_and(|num_end| {
                    let mut r = num_end;
                    while bytes.get(r).copied().is_some_and(is_space) {
                        r += 1;
                    }
                    r > num_end && bytes.get(r) == Some(&b'R')
                });
            if is_reference {
                out[key_at + KEY.len() - 1] = b'x';
                renamed = true;
            }
        }
    }
    renamed.then_some(out)
}

/// Un document `lopdf` le temps d'une passe : déchiffré puis rechiffré à l'identique, ou, pour
/// des filtres `Identity`, relu en **passe-plat** (ni déchiffré ni rechiffré, jamais renuméroté).
pub(crate) struct LopdfSession {
    /// Le document, objets en clair (en passe-plat : tels que lus, seules les pièces jointes
    /// restant chiffrées).
    pub(crate) doc: LoDocument,
    /// L'état de chiffrement d'origine (`None` : document non chiffré).
    state: Option<EncryptionState>,
    /// Vrai pour un fichier lu en passe-plat : ses objets chiffrés (pièces jointes) sont
    /// chiffrés **avec leur numéro d'objet** dans la clé (RC4 et AES, §7.6.2) ; les renuméroter
    /// les rendrait illisibles. Voir [`LopdfSession::keeps_object_numbers`].
    passthrough: bool,
}

impl LopdfSession {
    /// Vrai si la passe ne doit **pas** renuméroter les objets (`renumber_objects`) : élaguer
    /// reste permis, déplacer un numéro non.
    pub(crate) fn keeps_object_numbers(&self) -> bool {
        self.passthrough
    }

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
        Ok(LopdfSession {
            doc,
            state,
            passthrough: false,
        })
    }

    /// Session d'un fichier `/StmF /StrF Identity` : relu sans déchiffrement (la première
    /// lecture a déjà authentifié le mot de passe), avec `/Encrypt` remis dans la remorque pour
    /// que l'élagage le garde ; `/ID` n'est pas touché, et les objets ne doivent pas être
    /// renumérotés (`keeps_object_numbers`). `finish` n'a rien à rechiffrer : ce qui est
    /// chiffré (les pièces jointes) n'a jamais été lu en clair.
    fn passthrough(state: &EncryptionState, bytes: &[u8], repair: bool) -> CoreResult<Self> {
        let hidden = hide_encrypt_entry(bytes).ok_or_else(refuse)?;
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
        // La clé rebaptisée n'a plus de raison d'être : elle ne doit pas se retrouver écrite.
        doc.trailer.remove(b"Encrypx");
        doc.trailer.set("Encrypt", Object::Reference(id));
        Ok(LopdfSession {
            doc,
            state: None,
            passthrough: true,
        })
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

/// Débuts possibles d'un paquet XMP **en clair** : l'enveloppe `<?xpacket … ?>` est
/// facultative (ISO 16684-1, §7.3), le paquet peut commencer directement par `<x:xmpmeta`.
const XMP_STARTS: [&[u8]; 2] = [b"<?xpacket", b"<x:xmpmeta"];

/// Vrai si `data` commence, après d'éventuels blancs, par un paquet XMP lisible : du XML en
/// clair. Un flux chiffré (ou abîmé par une relecture antérieure) n'en a que la probabilité
/// d'octets aléatoires, nulle en pratique.
fn is_clear_xmp(data: &[u8]) -> bool {
    let start = data
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(data.len());
    XMP_STARTS
        .iter()
        .any(|marker| data[start..].starts_with(marker))
}

/// Si le flux `stream` est un XMP lisible **sans déchiffrement**, le contenu qu'il doit porter
/// avant d'être chiffré ; `None` sinon (déjà chiffré, ou abîmé). Trois formes :
///
/// 1. du XML en clair : ce contenu tel quel ;
/// 2. des octets compressés en clair, `/Filter` déclaré : ce contenu tel quel (le filtre
///    s'applique après déchiffrement, §7.6.2) ;
/// 3. des octets Flate en clair **sans** `/Filter` : PDFium retire le filtre d'un XMP
///    compressé mais en recopie les octets compressés (mesuré, tâche 21b ; qpdf compresse
///    par défaut les flux qui ne l'étaient pas, XMP compris). Le contenu rendu est alors le
///    XMP décompressé, la forme que PDFium voulait écrire.
///
/// Des octets chiffrés ne se décompressent pas en XML ; au pire, un décodeur tolérant rend des
/// octets aléatoires, refusés eux aussi.
fn clear_xmp_content(stream: &Stream) -> Option<Vec<u8>> {
    if is_clear_xmp(&stream.content) {
        return Some(stream.content.clone());
    }
    let filtered = stream.filters().is_ok_and(|filters| !filters.is_empty());
    if filtered {
        let decoded = stream
            .decompressed_content_with_limit(MAX_DECOMPRESSED)
            .ok()?;
        return is_clear_xmp(&decoded).then(|| stream.content.clone());
    }
    // Forme 3 : on essaie Flate sur une copie à laquelle on rend son filtre.
    let mut probe = stream.clone();
    probe
        .dict
        .set("Filter", Object::Name(b"FlateDecode".to_vec()));
    let inflated = probe
        .decompressed_content_with_limit(MAX_DECOMPRESSED)
        .ok()?;
    is_clear_xmp(&inflated).then_some(inflated)
}

/// Vrai si `needle` apparaît dans `haystack`.
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Vrai si la **dernière** remorque classique de `bytes` (entre le mot-clé `trailer` et
/// `startxref`) déclare `/Encrypt`. Ne lit que la fin du fichier, où PDFium l'écrit : un
/// document non chiffré passe ce contrôle en quelques centaines d'octets.
///
/// Sans remorque classique (table de références en flux), `false` : PDFium écrit toujours une
/// table classique, et ce contrôle ne sert qu'à ses sorties.
fn trailer_declares_encrypt(bytes: &[u8]) -> bool {
    // [Apprentissage] `rposition` cherche depuis la **fin** : `windows(7)` sait se parcourir
    // à l'envers (`DoubleEndedIterator`) et connaît sa longueur (`ExactSizeIterator`).
    let Some(at) = bytes.windows(7).rposition(|w| w == b"trailer") else {
        return false;
    };
    let rest = &bytes[at..];
    let end = rest
        .windows(9)
        .position(|w| w == b"startxref")
        .unwrap_or(rest.len());
    contains(&rest[..end], b"/Encrypt")
}

/// Vrai si le dictionnaire `/Encrypt` laisse les métadonnées en clair : `/EncryptMetadata
/// false`, ou flux en clair par construction (`/StmF` absent ou `Identity`). Ces deux clés
/// n'existent qu'à partir de la version 4 ; avant, tout flux est chiffré en RC4. Lu avant la
/// dérivation de la clé, qui coûte cher en révision 6.
fn metadata_left_clear(encrypt: &lopdf::Dictionary) -> bool {
    let version = encrypt.get(b"V").and_then(Object::as_i64).unwrap_or(0);
    if version < 4 {
        return false;
    }
    let declared_clear = matches!(
        encrypt.get(b"EncryptMetadata").and_then(Object::as_bool),
        Ok(false)
    );
    declared_clear || is_identity(encrypt.get(b"StmF").and_then(Object::as_name).ok())
}

/// Erreur d'un rechiffrement du XMP impossible (jamais montrée telle quelle à l'utilisateur :
/// [`seal_metadata`] la note au journal et garde les octets de PDFium).
fn metadata_error(detail: &str) -> CoreError {
    CoreError::InvalidOperation(format!("rechiffrement du XMP : {detail}"))
}

/// Rechiffre le XMP que PDFium a écrit **en clair** dans un document chiffré (constat F21-4,
/// tâche 21b du Lot 14). `Ok(Some(octets corrigés))`, ou `Ok(None)` s'il n'y a rien à faire :
/// document non chiffré, aucun XMP en clair, XMP déclaré en clair (`/EncryptMetadata false`)
/// ou flux en clair par construction (filtre `Identity`).
///
/// Pourquoi : PDFium écrit toujours le flux `/Metadata` en clair, sans poser
/// `/EncryptMetadata false` ; à la relecture (chaque enregistrement, chaque instantané
/// d'historique relit sa sortie), il le « déchiffre » en octets aléatoires. Seuls ces flux
/// sont touchés : chiffrés avec la clé du fichier et le filtre de flux déclaré (`/StmF`, RC4
/// avant la version 4), sous **leur** numéro d'objet, comme l'exige §7.6.2.
///
/// Le fichier est lu sans être déchiffré (`/Encrypt` masqué, comme pour une session
/// passe-plat) : aucun autre objet n'est déchiffré ni rechiffré, aucun n'est renuméroté. Un
/// flux qui n'est pas du XML lisible sans clé, compressé ou non (déjà chiffré, ou abîmé par une
/// relecture antérieure), est laissé tel quel : jamais chiffré deux fois. Tous les flux
/// `/Type /Metadata` sont concernés, pas seulement celui du catalogue : lopdf, comme PDFium,
/// applique `/EncryptMetadata` à chacun.
pub(crate) fn reencrypt_metadata(
    bytes: &[u8],
    password: Option<&str>,
) -> CoreResult<Option<Vec<u8>>> {
    // Les dictionnaires ne sont jamais chiffrés : sans le nom `/Metadata` en clair dans le
    // fichier, aucun flux de métadonnées, rien à lire.
    if !trailer_declares_encrypt(bytes) || !contains(bytes, b"/Metadata") {
        return Ok(None);
    }
    let hidden = hide_encrypt_entry(bytes)
        .ok_or_else(|| metadata_error("remorque sans référence /Encrypt"))?;
    let mut doc = load_limited(&hidden, None)
        .map_err(|e| metadata_error(&format!("structure illisible ({e})")))?;
    let encrypt_id = doc
        .trailer
        .get(b"Encrypx")
        .and_then(Object::as_reference)
        .map_err(|_| metadata_error("référence /Encrypt illisible"))?;
    doc.trailer.remove(b"Encrypx");
    doc.trailer.set("Encrypt", Object::Reference(encrypt_id));
    // Chaque flux à chiffrer et le contenu en clair qu'il doit porter.
    let targets: Vec<(ObjectId, Vec<u8>)> = doc
        .objects
        .iter()
        .filter_map(|(id, object)| {
            let stream = object.as_stream().ok()?;
            let is_metadata = stream.dict.get(b"Type").and_then(Object::as_name).ok()
                == Some(b"Metadata".as_slice());
            // Un filtre `/Crypt` propre au flux choisit lui-même son chiffrement : pas le nôtre.
            let own_crypt = stream
                .filters()
                .is_ok_and(|filters| filters.contains(&b"Crypt".as_slice()));
            if !is_metadata || own_crypt {
                return None;
            }
            clear_xmp_content(stream).map(|content| (*id, content))
        })
        .collect();
    let encrypt = doc
        .get_dictionary(encrypt_id)
        .map_err(|_| metadata_error("dictionnaire /Encrypt illisible"))?;
    if targets.is_empty() || metadata_left_clear(encrypt) {
        return Ok(None);
    }
    let password = password.unwrap_or("");
    let algorithm = lopdf::encryption::PasswordAlgorithm::try_from(&doc)
        .map_err(|e| metadata_error(&format!("chiffrement illisible ({e})")))?;
    let sanitized = algorithm
        .sanitize_password(password)
        .map_err(|e| metadata_error(&format!("mot de passe ({e})")))?;
    // `decode` vérifie le mot de passe (utilisateur ou propriétaire) avant de calculer la clé :
    // un mot de passe faux est une erreur, jamais une clé fausse qui rendrait le XMP chiffré
    // illisible pour toujours (une seule dérivation, coûteuse en révision 6).
    let state = EncryptionState::decode(&doc, &sanitized)
        .map_err(|e| metadata_error(&format!("clé du fichier ({e})")))?;
    let filter = state.get_stream_filter();
    for (id, content) in targets {
        let key = filter
            .compute_key(state.file_encryption_key(), id)
            .map_err(|e| metadata_error(&format!("clé de l'objet ({e})")))?;
        if let Some(Object::Stream(stream)) = doc.objects.get_mut(&id) {
            let sealed = filter
                .encrypt(&key, &content)
                .map_err(|e| metadata_error(&format!("chiffrement ({e})")))?;
            stream.set_content(sealed);
        }
    }
    let mut out = Vec::new();
    doc.save_to(&mut out)?;
    Ok(Some(out))
}

/// Les octets que PDFium vient d'écrire, XMP rechiffré si besoin ([`reencrypt_metadata`]).
/// Un échec ne bloque jamais un enregistrement : les octets de PDFium sont rendus tels quels
/// (le XMP sera perdu, comme avant la correction) et l'échec est noté au journal de
/// diagnostic.
pub(crate) fn seal_metadata(bytes: Vec<u8>, password: Option<&str>) -> Vec<u8> {
    match reencrypt_metadata(&bytes, password) {
        Ok(Some(sealed)) => sealed,
        Ok(None) => bytes,
        Err(e) => {
            crate::diagnostic::warn(&format!("XMP laissé en clair par PDFium : {e}"));
            bytes
        }
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
    parse_version(head.get(start..)?)
}

/// `M.m…` → `(M, m)` : les deux chiffres d'une version PDF (chiffres uniques, comme le sont
/// toutes les versions PDF ; ce qui suit est ignoré). Sert à l'en-tête (`%PDF-1.7`), au
/// `/Version` du catalogue (un nom, `/1.7`) et à la version d'un document `lopdf` (`"1.7"`) :
/// un seul analyseur pour les trois.
///
/// \[Apprentissage\] Les tuples se comparent champ par champ : `(1, 4) < (1, 7) < (2, 0)` ;
/// le couple dit ce que l'on compare, là où une chaîne comparerait des caractères.
pub(crate) fn parse_version(text: &[u8]) -> Option<(u8, u8)> {
    match text {
        [major @ b'0'..=b'9', b'.', minor @ b'0'..=b'9', ..] => Some((major - b'0', minor - b'0')),
        _ => None,
    }
}

/// Copie de `bytes` dont l'en-tête annonce au moins `min_version`, sans changer la longueur
/// (les trois octets `M.n` sont réécrits en place : tous les décalages restent exacts).
/// Les octets sont rendus tels quels si l'en-tête est déjà assez récent ou introuvable.
pub(crate) fn raise_header_version(bytes: &[u8], min_version: Option<(u8, u8)>) -> Vec<u8> {
    let mut out = bytes.to_vec();
    if let (Some(min), Some(current)) = (min_version, header_version(bytes))
        // Comparaison de tuples, champ par champ (voir `parse_version`).
        && min > current
        // Une composante à deux chiffres ne tient pas dans un octet `M` ou `n`.
        && min.0 <= 9
        && min.1 <= 9
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

    /// Revue du Lot 14 (M1) : une version à deux chiffres ne s'écrit pas dans les trois octets
    /// `M.n` (`b'0' + 12` n'est pas un chiffre) ; l'en-tête reste tel quel.
    #[test]
    fn raise_header_version_ignores_a_multi_digit_component() {
        assert_eq!(
            raise_header_version(b"%PDF-1.4\nx", Some((1, 12))),
            b"%PDF-1.4\nx"
        );
        assert_eq!(
            raise_header_version(b"%PDF-1.4\nx", Some((250, 0))),
            b"%PDF-1.4\nx"
        );
    }
}

#[cfg(test)]
mod hide_tests {
    use super::*;

    #[test]
    fn only_the_trailer_reference_is_renamed() {
        let input = b"%PDF-1.6\n1 0 obj\n(/Encrypt 12 0 R)\nendobj\ntrailer\n<< /Size 3 /Encrypt 6 0 R /ID [<aa><bb>] >>\nstartxref\n0\n%%EOF\n";
        let out = hide_encrypt_entry(input).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("(/Encrypt 12 0 R)"),
            "texte hors remorque intact"
        );
        assert!(text.contains("/Encrypx 6 0 R"));
        assert_eq!(text.len(), input.len());
    }

    #[test]
    fn no_trailer_means_no_passthrough() {
        assert!(hide_encrypt_entry(b"%PDF-1.6\n<< /Encrypt 6 0 R >>\n").is_none());
        assert!(hide_encrypt_entry(b"trailer\n<< /Size 3 >>\n").is_none());
    }
}

#[cfg(test)]
mod metadata_tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
    use lopdf::encryption::{EncryptionVersion, Permissions};

    use super::*;
    use crate::Document;

    /// `rich_catalog_pdf` (XMP « Riche ») chiffré par lopdf en AES-256, mot de passe `u` ;
    /// `encrypt_metadata` dit si le XMP est chiffré lui aussi, `compress` si les flux sont
    /// compressés (comme le fait qpdf), `page_xmp` si la page 1 porte son propre XMP (titre
    /// « PageXmp »).
    fn encrypted_rich(encrypt_metadata: bool, compress: bool, page_xmp: bool) -> Vec<u8> {
        let mut doc = LoDocument::load_mem(&crate::testing::rich_catalog_pdf("1.7", None)).unwrap();
        if page_xmp {
            let xmp = "<?xpacket begin=\"\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\
                <x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF \
                xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
                <rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
                <dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">PageXmp</rdf:li></rdf:Alt>\
                </dc:title></rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>";
            let mut dict = lopdf::Dictionary::new();
            dict.set("Type", Object::Name(b"Metadata".to_vec()));
            dict.set("Subtype", Object::Name(b"XML".to_vec()));
            let id = doc.add_object(Stream::new(dict, xmp.as_bytes().to_vec()));
            let page = doc.page_iter().next().unwrap();
            doc.get_dictionary_mut(page)
                .unwrap()
                .set("Metadata", Object::Reference(id));
        }
        if compress {
            doc.compress();
        }
        let mut filters: BTreeMap<Vec<u8>, Arc<dyn CryptFilter>> = BTreeMap::new();
        filters.insert(b"StdCF".to_vec(), Arc::new(Aes256CryptFilter));
        let state = EncryptionState::try_from(EncryptionVersion::V5 {
            encrypt_metadata,
            crypt_filters: filters,
            file_encryption_key: &[3u8; 32],
            stream_filter: b"StdCF".to_vec(),
            string_filter: b"StdCF".to_vec(),
            owner_password: "o",
            user_password: "u",
            permissions: Permissions::all(),
        })
        .unwrap();
        doc.encrypt(&state).unwrap();
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    /// Ce qu'écrit PDFium pour `bytes`, sans la correction de `Document::to_bytes`.
    fn pdfium_output(bytes: Vec<u8>, password: &str) -> Vec<u8> {
        let doc = Document::from_bytes(bytes, Some(password)).unwrap();
        let _guard = crate::engine::guard();
        doc.inner().save_to_bytes().unwrap()
    }

    /// Le XMP du catalogue, déchiffré avec `password`.
    fn catalog_xmp(bytes: &[u8], password: &str) -> Vec<u8> {
        let doc = load_limited(bytes, Some(password)).unwrap();
        let id = doc
            .catalog()
            .unwrap()
            .get(b"Metadata")
            .unwrap()
            .as_reference()
            .unwrap();
        doc.get_object(id)
            .unwrap()
            .as_stream()
            .unwrap()
            .content
            .clone()
    }

    fn has_clear_xmp(bytes: &[u8]) -> bool {
        bytes.windows(10).any(|w| w == b"<x:xmpmeta")
    }

    #[test]
    fn clear_xmp_of_an_encrypted_output_is_encrypted_again_once() {
        let out = pdfium_output(encrypted_rich(true, false, false), "u");
        assert!(
            has_clear_xmp(&out),
            "constat F21-4 : PDFium écrit le XMP en clair"
        );
        let fixed = reencrypt_metadata(&out, Some("u"))
            .unwrap()
            .expect("XMP rechiffré");
        assert!(!has_clear_xmp(&fixed), "plus de XMP en clair");
        let xmp = catalog_xmp(&fixed, "u");
        assert!(
            xmp.starts_with(b"<?xpacket") && xmp.windows(5).any(|w| w == b"Riche"),
            "XMP relu : {:?}",
            String::from_utf8_lossy(&xmp)
                .chars()
                .take(40)
                .collect::<String>()
        );
        let reopened = Document::from_bytes(fixed.clone(), Some("u")).unwrap();
        assert_eq!(reopened.page_count(), 3);
        // Jamais deux fois : un XMP déjà chiffré n'est pas du XML en clair, on n'y touche pas.
        assert!(reencrypt_metadata(&fixed, Some("u")).unwrap().is_none());
    }

    /// qpdf compresse le XMP par défaut : PDFium le réécrit compressé **et** en clair, des
    /// octets Flate qui ne commencent pas par `<?xpacket` mais se décompressent en XMP.
    #[test]
    fn compressed_clear_xmp_is_encrypted_again() {
        let out = pdfium_output(encrypted_rich(true, true, false), "u");
        let fixed = reencrypt_metadata(&out, Some("u"))
            .unwrap()
            .expect("XMP compressé rechiffré");
        let xmp = catalog_xmp(&fixed, "u");
        assert!(
            xmp.starts_with(b"<?xpacket") && xmp.windows(5).any(|w| w == b"Riche"),
            "XMP relu : {:?}",
            String::from_utf8_lossy(&xmp)
                .chars()
                .take(40)
                .collect::<String>()
        );
        assert!(reencrypt_metadata(&fixed, Some("u")).unwrap().is_none());
    }

    #[test]
    fn nothing_to_do_without_encryption_or_with_a_declared_cleartext_xmp() {
        let plain = crate::testing::rich_catalog_pdf("1.7", None);
        assert!(reencrypt_metadata(&plain, None).unwrap().is_none());
        let declared_clear = pdfium_output(encrypted_rich(false, false, false), "u");
        assert!(has_clear_xmp(&declared_clear));
        assert!(
            reencrypt_metadata(&declared_clear, Some("u"))
                .unwrap()
                .is_none(),
            "/EncryptMetadata false : le XMP reste en clair"
        );
    }

    #[test]
    fn a_wrong_password_is_an_error() {
        let out = pdfium_output(encrypted_rich(true, false, false), "u");
        assert!(reencrypt_metadata(&out, Some("faux")).is_err());
    }

    /// Mesuré (tâche 21b) : PDFium écrit en clair **tout** flux `/Type /Metadata`, celui d'une
    /// page compris, et le déchiffre à la relecture. Il est rechiffré comme celui du catalogue
    /// et survit à deux enregistrements.
    #[test]
    fn a_page_level_xmp_is_encrypted_again_too() {
        let source = encrypted_rich(true, false, true);
        let has_page_xmp = |bytes: &[u8]| bytes.windows(7).any(|w| w == b"PageXmp");
        assert!(has_page_xmp(&pdfium_output(source.clone(), "u")));
        let mut doc = Document::from_bytes(source, Some("u")).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("page.pdf");
        for round in 0..2 {
            doc.rotate_pages(&[0], 1).unwrap();
            doc.save_as(&path).unwrap();
            let out = std::fs::read(&path).unwrap();
            assert!(!has_page_xmp(&out), "tour {round} : XMP de page en clair");
            let read = load_limited(&out, Some("u")).unwrap();
            let page = read.page_iter().next().unwrap();
            let id = read
                .get_dictionary(page)
                .unwrap()
                .get(b"Metadata")
                .unwrap()
                .as_reference()
                .unwrap();
            let xmp = stream_bytes(read.get_object(id).unwrap().as_stream().unwrap()).unwrap();
            assert!(has_page_xmp(&xmp), "tour {round} : XMP de page détruit");
        }
    }
}
