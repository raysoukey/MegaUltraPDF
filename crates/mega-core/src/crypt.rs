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

use std::sync::Arc;

use lopdf::encryption::EncryptionState;
use lopdf::encryption::crypt_filters::{CryptFilter, Rc4CryptFilter};
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
            return Self::passthrough(bytes, repair);
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
    fn passthrough(bytes: &[u8], repair: bool) -> CoreResult<Self> {
        let (doc, id) = load_undecrypted(bytes, repair)?;
        let is_standard = doc
            .get_dictionary(id)
            .ok()
            .and_then(|d| d.get(b"Filter").and_then(Object::as_name).ok())
            == Some(b"Standard".as_slice());
        if !is_standard {
            return Err(refuse());
        }
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

/// Marques d'un paquet XMP **en clair** : l'enveloppe `<?xpacket … ?>` est facultative
/// (ISO 16684-1, §7.3), le paquet peut commencer par `<x:xmpmeta`, ou même directement par
/// `<rdf:RDF`.
const XMP_MARKERS: [&[u8]; 3] = [b"<?xpacket", b"<x:xmpmeta", b"<rdf:RDF"];

/// Nombre d'octets lus au début d'un flux pour y chercher une marque XMP : un prologue
/// `<?xml …?>`, des commentaires ou des blancs peuvent la précéder.
const XMP_HEAD: usize = 1024;

/// Marque d'ordre des octets UTF-8, que certains producteurs placent devant le paquet.
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// Taille décompressée maximale d'un XMP examiné ou décompressé ici (16 Mio) : un XMP réel
/// pèse quelques Ko, au pire quelques centaines (vignettes incorporées).
const XMP_LIMIT: usize = 16 << 20;

/// Vrai si `data` est un paquet XMP lisible : du XML en clair, dont le premier Kio (après une
/// éventuelle marque d'ordre UTF-8) contient une marque de [`XMP_MARKERS`]. Un flux chiffré
/// (ou abîmé par une relecture antérieure) n'en a que la probabilité d'octets aléatoires,
/// nulle en pratique.
///
/// Limite assumée : un XMP en UTF-16 ou UTF-32 (permis par ISO 16684-1, jamais rencontré dans
/// un PDF) n'est pas reconnu ; il serait laissé tel quel, comme avant la tâche 21b.
pub(crate) fn is_clear_xmp(data: &[u8]) -> bool {
    let data = data.strip_prefix(UTF8_BOM).unwrap_or(data);
    let head = &data[..data.len().min(XMP_HEAD)];
    XMP_MARKERS.iter().any(|marker| contains(head, marker))
}

/// Si le flux `stream` est un XMP lisible **sans déchiffrement**, ce qu'il faut en faire ;
/// `None` sinon (déjà chiffré, ou abîmé). Trois formes :
///
/// 1. du XML en clair : [`ClearXmp::AsIs`] ;
/// 2. des octets compressés en clair, `/Filter` déclaré : [`ClearXmp::AsIs`] aussi (le
///    filtre s'applique après déchiffrement, §7.6.2) ;
/// 3. des octets Flate en clair **sans** `/Filter` : PDFium retire le filtre d'un XMP
///    compressé mais en recopie les octets compressés (mesuré, tâche 21b ; qpdf compresse les
///    flux quand il chiffre, XMP compris). [`ClearXmp::Inflated`] porte alors le XMP
///    décompressé, la forme que PDFium voulait écrire.
///
/// Des octets chiffrés ne se décompressent pas en XML ; au pire, un décodeur tolérant rend des
/// octets aléatoires, refusés eux aussi.
fn clear_xmp_content(stream: &Stream) -> Option<ClearXmp> {
    if is_clear_xmp(&stream.content) {
        return Some(ClearXmp::AsIs);
    }
    let filtered = stream.filters().is_ok_and(|filters| !filters.is_empty());
    if filtered {
        let decoded = stream_bytes_within(stream, XMP_LIMIT)?;
        return is_clear_xmp(&decoded).then_some(ClearXmp::AsIs);
    }
    // Forme 3 : on essaie Flate sur une copie à laquelle on rend son filtre.
    let mut probe = stream.clone();
    probe
        .dict
        .set("Filter", Object::Name(b"FlateDecode".to_vec()));
    let inflated = stream_bytes_within(&probe, XMP_LIMIT)?;
    is_clear_xmp(&inflated).then_some(ClearXmp::Inflated(inflated))
}

/// Un flux de métadonnées lisible sans clé (voir [`clear_xmp_content`]).
enum ClearXmp {
    /// Son contenu est bon tel quel (XML en clair, ou compressé avec son `/Filter`).
    AsIs,
    /// PDFium a retiré son `/Filter` : voici le XMP décompressé à écrire à la place.
    Inflated(Vec<u8>),
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

/// Ce que le dictionnaire `/Encrypt` impose aux flux de métadonnées.
enum MetadataCipher {
    /// Laissés en clair : `/EncryptMetadata false`, ou filtre de flux `Identity` (`/StmF`
    /// absent, nommé `Identity`, ou entrée de `/CF` de méthode `Identity`).
    Clear,
    /// Chiffrés par ce filtre : RC4 avant la version 4, l'entrée `/StmF` de `/CF` ensuite.
    ///
    /// \[Apprentissage\] **`Arc<dyn CryptFilter>`, un objet-trait** : `dyn CryptFilter` désigne
    /// « une valeur d'un type qui implémente le trait `CryptFilter` », sans dire lequel. Le
    /// choix se fait **à l'exécution**, d'après le fichier (RC4, AES-128 ou AES-256), et les
    /// appels `compute_key` / `encrypt` vont à l'implémentation de ce type-là : l'algorithme
    /// de clé de chaque objet change avec lui. `Arc` partage la valeur sans la copier (lopdf
    /// rend ses filtres sous cette forme).
    Encrypt(Arc<dyn CryptFilter>),
}

/// Lit dans `/Encrypt` le sort des flux de métadonnées, **avant** la dérivation de la clé (qui
/// coûte cher en révision 6). `/EncryptMetadata` et `/StmF` n'existent qu'à partir de la
/// version 4 ; avant, tout flux est chiffré en RC4.
///
/// Un `/StmF` qui n'est pas une entrée de `/CF` que lopdf sait employer (entrée absente, ou
/// méthode `/CFM` inconnue de lui, comme `/None`) est une erreur : se rabattre sur RC4, comme
/// le fait lopdf, chiffrerait le XMP d'une façon qu'aucun lecteur ne déferait.
fn metadata_cipher(doc: &LoDocument, encrypt: &lopdf::Dictionary) -> CoreResult<MetadataCipher> {
    let version = encrypt.get(b"V").and_then(Object::as_i64).unwrap_or(0);
    if version < 4 {
        return Ok(MetadataCipher::Encrypt(Arc::new(Rc4CryptFilter)));
    }
    let declared_clear = matches!(
        encrypt.get(b"EncryptMetadata").and_then(Object::as_bool),
        Ok(false)
    );
    let name = encrypt.get(b"StmF").and_then(Object::as_name).ok();
    if declared_clear || is_identity(name) {
        return Ok(MetadataCipher::Clear);
    }
    let filter = name
        .and_then(|name| doc.get_crypt_filters().get(name).cloned())
        .ok_or_else(|| {
            metadata_error("filtre de flux /StmF absent de /CF ou non pris en charge")
        })?;
    if filter.method() == b"Identity" {
        return Ok(MetadataCipher::Clear);
    }
    Ok(MetadataCipher::Encrypt(filter))
}

/// Erreur d'un rechiffrement du XMP impossible (jamais montrée telle quelle à l'utilisateur :
/// [`seal_metadata`] la note au journal et garde les octets de PDFium).
fn metadata_error(detail: &str) -> CoreError {
    CoreError::UnsupportedEncryption(format!("rechiffrement du XMP : {detail}"))
}

/// Lit `bytes` **sans rien déchiffrer** : la référence `/Encrypt` de la remorque est masquée
/// ([`hide_encrypt_entry`]) le temps de la lecture, puis remise en mémoire. Rend le document
/// (objets tels qu'écrits, chaînes et flux encore chiffrés) et le numéro du dictionnaire
/// `/Encrypt`. `repair` choisit [`load_limited`] (remorque réparée) ou [`load_exact`].
///
/// Sert à la session passe-plat (filtres `Identity`) et au rechiffrement du XMP : deux passes
/// qui ne doivent toucher qu'à quelques objets, jamais déchiffrer tout le fichier.
fn load_undecrypted(bytes: &[u8], repair: bool) -> CoreResult<(LoDocument, ObjectId)> {
    let hidden = hide_encrypt_entry(bytes).ok_or_else(refuse)?;
    let loaded = if repair {
        load_limited(&hidden, None)
    } else {
        load_exact(&hidden, None)
    };
    let mut doc = loaded
        .map_err(|e| CoreError::InvalidOperation(format!("structure du PDF illisible : {e}")))?;
    let id = doc
        .trailer
        .get(b"Encrypx")
        .and_then(Object::as_reference)
        .map_err(|_| refuse())?;
    // La clé rebaptisée n'a plus de raison d'être : elle ne doit pas se retrouver écrite.
    doc.trailer.remove(b"Encrypx");
    doc.trailer.set("Encrypt", Object::Reference(id));
    Ok((doc, id))
}

/// Rechiffre le XMP que PDFium a écrit **en clair** dans un document chiffré (constat F21-4,
/// tâche 21b du Lot 14). `Ok(Some(octets corrigés))`, ou `Ok(None)` s'il n'y a rien à faire :
/// document non chiffré, aucun XMP en clair, XMP déclaré en clair (`/EncryptMetadata false`)
/// ou flux en clair par construction (filtre `Identity`).
///
/// Pourquoi : PDFium écrit toujours les flux `/Metadata` en clair, sans poser
/// `/EncryptMetadata false` ; à la relecture (chaque enregistrement, chaque instantané
/// d'historique relit sa sortie), il les « déchiffre » en octets aléatoires. Seuls ces flux
/// sont touchés, chiffrés avec le filtre de flux déclaré (`/StmF`, RC4 avant la version 4).
/// En RC4 et en AES-128, la clé de chaque objet mêle la clé du fichier à **son** numéro
/// d'objet (§7.6.2, algorithme 1) ; en AES-256, la clé du fichier sert telle quelle
/// (algorithme 1.A).
///
/// Le fichier est lu sans être déchiffré ([`load_undecrypted`]) : aucun autre objet n'est
/// déchiffré ni rechiffré, aucun n'est renuméroté. Un flux qui n'est pas du XML lisible sans
/// clé, compressé ou non (déjà chiffré, ou abîmé par une relecture antérieure), est laissé tel
/// quel : jamais chiffré deux fois. Tous les flux `/Type /Metadata` sont concernés, pas
/// seulement celui du catalogue : PDFium les écrit tous en clair (mesuré).
pub(crate) fn reencrypt_metadata(
    bytes: &[u8],
    password: Option<&str>,
) -> CoreResult<Option<Vec<u8>>> {
    // Les dictionnaires ne sont jamais chiffrés : sans le nom `/Metadata` en clair dans le
    // fichier, aucun flux de métadonnées, rien à lire.
    if !trailer_declares_encrypt(bytes) || !contains(bytes, b"/Metadata") {
        return Ok(None);
    }
    let (mut doc, encrypt_id) = load_undecrypted(bytes, true)?;
    let encrypt = doc
        .get_dictionary(encrypt_id)
        .map_err(|_| metadata_error("dictionnaire /Encrypt illisible"))?;
    let MetadataCipher::Encrypt(filter) = metadata_cipher(&doc, encrypt)? else {
        return Ok(None);
    };
    // Chaque flux à chiffrer et le contenu en clair qu'il doit porter.
    let targets: Vec<(ObjectId, Option<Vec<u8>>)> = doc
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
            match clear_xmp_content(stream)? {
                ClearXmp::AsIs => Some((*id, None)),
                ClearXmp::Inflated(xmp) => Some((*id, Some(xmp))),
            }
        })
        .collect();
    if targets.is_empty() {
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
    for (id, inflated) in targets {
        let key = filter
            .compute_key(state.file_encryption_key(), id)
            .map_err(|e| metadata_error(&format!("clé de l'objet ({e})")))?;
        if let Some(Object::Stream(stream)) = doc.objects.get_mut(&id) {
            let clear = inflated.as_deref().unwrap_or(&stream.content);
            let sealed = filter
                .encrypt(&key, clear)
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

    use lopdf::encryption::crypt_filters::{Aes128CryptFilter, Aes256CryptFilter, CryptFilter};
    use lopdf::encryption::{EncryptionVersion, Permissions};

    use super::*;
    use crate::Document;

    /// Chiffrement du fichier source.
    #[derive(Clone, Copy, Debug)]
    enum Cipher {
        Aes256,
        Aes128,
        Rc4,
    }

    /// Réglages du fichier source : `rich_catalog_pdf` (XMP « Riche ») chiffré par lopdf,
    /// mots de passe `u` (utilisateur) et `o` (propriétaire).
    #[derive(Clone, Copy)]
    struct Source {
        cipher: Cipher,
        /// Le XMP est-il chiffré (`/EncryptMetadata`) ?
        encrypt_metadata: bool,
        /// Flux compressés, comme le fait qpdf en chiffrant ?
        compress: bool,
        /// La page 1 porte-t-elle son propre XMP (titre « PageXmp ») ?
        page_xmp: bool,
        /// Octets placés devant le paquet XMP du catalogue (BOM, prologue XML).
        prefix: &'static [u8],
    }

    const AES256: Source = Source {
        cipher: Cipher::Aes256,
        encrypt_metadata: true,
        compress: false,
        page_xmp: false,
        prefix: b"",
    };

    const PAGE_XMP: &str = "<?xpacket begin=\"\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\
        <x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF \
        xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
        <rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
        <dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">PageXmp</rdf:li></rdf:Alt>\
        </dc:title></rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>";

    fn encrypted(source: Source) -> Vec<u8> {
        let mut doc = LoDocument::load_mem(&crate::testing::rich_catalog_pdf("1.7", None)).unwrap();
        if !source.prefix.is_empty() {
            let id = doc
                .catalog()
                .unwrap()
                .get(b"Metadata")
                .unwrap()
                .as_reference()
                .unwrap();
            let stream = doc.get_object_mut(id).unwrap().as_stream_mut().unwrap();
            let mut content = source.prefix.to_vec();
            content.extend_from_slice(&stream.content);
            stream.set_content(content);
        }
        if source.page_xmp {
            let mut dict = lopdf::Dictionary::new();
            dict.set("Type", Object::Name(b"Metadata".to_vec()));
            dict.set("Subtype", Object::Name(b"XML".to_vec()));
            let id = doc.add_object(Stream::new(dict, PAGE_XMP.as_bytes().to_vec()));
            let page = doc.page_iter().next().unwrap();
            doc.get_dictionary_mut(page)
                .unwrap()
                .set("Metadata", Object::Reference(id));
        }
        if source.compress {
            doc.compress();
        }
        if doc.trailer.get(b"ID").is_err() {
            let id = Object::String(
                b"0123456789abcdef".to_vec(),
                lopdf::StringFormat::Hexadecimal,
            );
            doc.trailer.set("ID", Object::Array(vec![id.clone(), id]));
        }
        let filters = |filter: Arc<dyn CryptFilter>| {
            let mut filters: BTreeMap<Vec<u8>, Arc<dyn CryptFilter>> = BTreeMap::new();
            filters.insert(b"StdCF".to_vec(), filter);
            filters
        };
        let version = match source.cipher {
            Cipher::Aes256 => EncryptionVersion::V5 {
                encrypt_metadata: source.encrypt_metadata,
                crypt_filters: filters(Arc::new(Aes256CryptFilter)),
                file_encryption_key: &[3u8; 32],
                stream_filter: b"StdCF".to_vec(),
                string_filter: b"StdCF".to_vec(),
                owner_password: "o",
                user_password: "u",
                permissions: Permissions::all(),
            },
            Cipher::Aes128 => EncryptionVersion::V4 {
                document: &doc,
                encrypt_metadata: source.encrypt_metadata,
                crypt_filters: filters(Arc::new(Aes128CryptFilter)),
                stream_filter: b"StdCF".to_vec(),
                string_filter: b"StdCF".to_vec(),
                owner_password: "o",
                user_password: "u",
                permissions: Permissions::all(),
            },
            Cipher::Rc4 => EncryptionVersion::V2 {
                document: &doc,
                owner_password: "o",
                user_password: "u",
                key_length: 128,
                permissions: Permissions::all(),
            },
        };
        let state = EncryptionState::try_from(version).unwrap();
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

    /// Le XMP du catalogue, déchiffré avec `password` et décompressé.
    fn catalog_xmp(bytes: &[u8], password: &str) -> Vec<u8> {
        let doc = load_limited(bytes, Some(password)).unwrap();
        let id = doc
            .catalog()
            .unwrap()
            .get(b"Metadata")
            .unwrap()
            .as_reference()
            .unwrap();
        stream_bytes(doc.get_object(id).unwrap().as_stream().unwrap()).unwrap()
    }

    fn has(bytes: &[u8], needle: &[u8]) -> bool {
        bytes.windows(needle.len()).any(|w| w == needle)
    }

    fn has_clear_xmp(bytes: &[u8]) -> bool {
        has(bytes, b"<x:xmpmeta")
    }

    /// Le XMP du catalogue de `fixed` se relit (titre « Riche »).
    fn assert_riche(fixed: &[u8], what: &str) {
        let xmp = catalog_xmp(fixed, "u");
        assert!(
            has(&xmp, b">Riche</rdf:li>"),
            "{what} : XMP relu {:?}",
            String::from_utf8_lossy(&xmp)
                .chars()
                .take(60)
                .collect::<String>()
        );
    }

    /// Copie de `bytes` dont le dictionnaire `/Encrypt` est modifié par `edit` (lu sans
    /// déchiffrement).
    fn with_encrypt_dict(bytes: &[u8], edit: impl Fn(&mut lopdf::Dictionary)) -> Vec<u8> {
        let hidden = hide_encrypt_entry(bytes).unwrap();
        let mut doc = load_limited(&hidden, None).unwrap();
        let id = doc.trailer.get(b"Encrypx").unwrap().as_reference().unwrap();
        doc.trailer.remove(b"Encrypx");
        doc.trailer.set("Encrypt", Object::Reference(id));
        edit(doc.get_dictionary_mut(id).unwrap());
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn clear_xmp_of_an_encrypted_output_is_encrypted_again_once() {
        let out = pdfium_output(encrypted(AES256), "u");
        assert!(
            has_clear_xmp(&out),
            "constat F21-4 : PDFium écrit le XMP en clair"
        );
        let fixed = reencrypt_metadata(&out, Some("u"))
            .unwrap()
            .expect("XMP rechiffré");
        assert!(!has_clear_xmp(&fixed), "plus de XMP en clair");
        assert_riche(&fixed, "AES-256");
        let reopened = Document::from_bytes(fixed.clone(), Some("u")).unwrap();
        assert_eq!(reopened.page_count(), 3);
        // Jamais deux fois : un XMP déjà chiffré n'est pas du XML en clair, on n'y touche pas.
        assert!(reencrypt_metadata(&fixed, Some("u")).unwrap().is_none());
    }

    /// Relecture de la tâche 21b : un XMP précédé d'un BOM UTF-8 ou d'un prologue `<?xml …?>`
    /// est du XMP en clair lui aussi.
    #[test]
    fn a_bom_or_an_xml_prologue_before_the_packet_is_recognized() {
        let prefixes: [&'static [u8]; 2] = [
            b"\xEF\xBB\xBF",
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        ];
        for cipher in [Cipher::Aes256, Cipher::Rc4] {
            for prefix in prefixes {
                let what = format!("{cipher:?}, préfixe {:?}", String::from_utf8_lossy(prefix));
                let source = Source {
                    cipher,
                    prefix,
                    ..AES256
                };
                let out = pdfium_output(encrypted(source), "u");
                let fixed = reencrypt_metadata(&out, Some("u"))
                    .unwrap()
                    .unwrap_or_else(|| panic!("{what} : XMP non reconnu"));
                assert!(!has_clear_xmp(&fixed), "{what}");
                assert_riche(&fixed, &what);
                assert!(catalog_xmp(&fixed, "u").starts_with(prefix), "{what}");
                assert!(reencrypt_metadata(&fixed, Some("u")).unwrap().is_none());
            }
        }
    }

    /// qpdf compresse les flux quand il chiffre : PDFium réécrit alors un XMP compressé **et**
    /// en clair, des octets Flate qui ne commencent pas par `<?xpacket` mais se décompressent
    /// en XMP.
    #[test]
    fn compressed_clear_xmp_is_encrypted_again() {
        let source = Source {
            compress: true,
            ..AES256
        };
        let out = pdfium_output(encrypted(source), "u");
        let fixed = reencrypt_metadata(&out, Some("u"))
            .unwrap()
            .expect("XMP compressé rechiffré");
        assert_riche(&fixed, "compressé");
        assert!(reencrypt_metadata(&fixed, Some("u")).unwrap().is_none());
    }

    #[test]
    fn nothing_to_do_without_encryption_or_with_a_declared_cleartext_xmp() {
        let plain = crate::testing::rich_catalog_pdf("1.7", None);
        assert!(reencrypt_metadata(&plain, None).unwrap().is_none());
        let source = Source {
            encrypt_metadata: false,
            ..AES256
        };
        let declared_clear = pdfium_output(encrypted(source), "u");
        assert!(has_clear_xmp(&declared_clear));
        assert!(
            reencrypt_metadata(&declared_clear, Some("u"))
                .unwrap()
                .is_none(),
            "/EncryptMetadata false : le XMP reste en clair"
        );
    }

    #[test]
    fn a_wrong_password_is_an_error_and_never_blocks_a_save() {
        let out = pdfium_output(encrypted(AES256), "u");
        assert!(reencrypt_metadata(&out, Some("faux")).is_err());
        // `seal_metadata` rend alors les octets de PDFium tels quels.
        assert_eq!(seal_metadata(out.clone(), Some("faux")), out);
    }

    /// Relecture de la tâche 21b : un `/StmF` absent de `/CF` ou une méthode `/CFM` que lopdf
    /// ne connaît pas (`/None`) ne retombent pas en silence sur RC4 (qui détruirait le XMP) :
    /// c'est une erreur, et les octets de PDFium restent tels quels.
    #[test]
    fn an_unknown_stream_filter_is_an_error() {
        let out = pdfium_output(
            encrypted(Source {
                cipher: Cipher::Aes128,
                ..AES256
            }),
            "u",
        );
        let missing = with_encrypt_dict(&out, |encrypt| {
            encrypt.set("StmF", Object::Name(b"Absent".to_vec()));
        });
        assert!(reencrypt_metadata(&missing, Some("u")).is_err());
        assert_eq!(seal_metadata(missing.clone(), Some("u")), missing);
        let cfm_none = with_encrypt_dict(&out, |encrypt| {
            let filters = encrypt.get_mut(b"CF").unwrap().as_dict_mut().unwrap();
            let std_cf = filters.get_mut(b"StdCF").unwrap().as_dict_mut().unwrap();
            std_cf.set("CFM", Object::Name(b"None".to_vec()));
        });
        assert!(reencrypt_metadata(&cfm_none, Some("u")).is_err());
    }

    /// Un filtre de flux dont la méthode est `/Identity` laisse les flux en clair : rien à
    /// chiffrer.
    #[test]
    fn an_identity_stream_filter_leaves_the_xmp_alone() {
        let out = pdfium_output(
            encrypted(Source {
                cipher: Cipher::Aes128,
                ..AES256
            }),
            "u",
        );
        let identity = with_encrypt_dict(&out, |encrypt| {
            let filters = encrypt.get_mut(b"CF").unwrap().as_dict_mut().unwrap();
            let std_cf = filters.get_mut(b"StdCF").unwrap().as_dict_mut().unwrap();
            std_cf.set("CFM", Object::Name(b"Identity".to_vec()));
        });
        assert!(reencrypt_metadata(&identity, Some("u")).unwrap().is_none());
    }

    /// Mesuré (tâche 21b) : PDFium écrit en clair **tout** flux `/Type /Metadata`, celui d'une
    /// page compris, et le déchiffre à la relecture. Il est rechiffré comme celui du catalogue
    /// et survit à deux enregistrements.
    #[test]
    fn a_page_level_xmp_is_encrypted_again_too() {
        let source = encrypted(Source {
            page_xmp: true,
            ..AES256
        });
        let has_page_xmp = |bytes: &[u8]| has(bytes, b"PageXmp");
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
