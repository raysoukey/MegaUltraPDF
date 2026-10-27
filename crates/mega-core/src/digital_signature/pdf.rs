// SPDX-License-Identifier: GPL-3.0-or-later
//! Signature d'un PDF par **mise à jour incrémentale**, et vérification des signatures.
//!
//! Signer, c'est ajouter à la fin du fichier, sans toucher aux octets existants :
//! - un dictionnaire de signature (`/Type /Sig`, `/SubFilter /adbe.pkcs7.detached`), dont
//!   `/ByteRange` désigne tout le fichier **sauf** la chaîne `/Contents`, et dont `/Contents`
//!   contient la structure CMS (module `cms`), complétée de zéros ;
//! - un champ de signature invisible (widget `/Rect [0 0 0 0]`) dans `/AcroForm` et dans
//!   les annotations de la première page ;
//! - une nouvelle table de références dont la remorque pointe vers l'ancienne (`/Prev`).
//!
//! Les signatures existantes restent donc intactes : elles couvrent toujours leurs octets.
//!
//! \[Apprentissage\] On écrit d'abord le fichier avec des **espaces réservés** de largeur
//! fixe (`/ByteRange [0 9999999999 …]`, `/Contents <000…>`), on calcule les décalages réels,
//! on les écrit **à la même largeur** (rien ne se décale), puis on signe l'empreinte des deux
//! plages et on recopie la signature en hexadécimal dans l'espace réservé. Des tranches
//! d'octets (`&out[..a]`, `&out[b..]`) et des indices : pas de magie.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use lopdf::xref::XrefEntry;
use lopdf::{
    Dictionary, Document as LoDocument, IncrementalDocument, Object, ObjectId, StringFormat,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{SigIssue, SignatureStatus, SigningIdentity, cms, sha256};
use crate::document::Document;
use crate::edit::write_atomically;
use crate::error::{CoreError, CoreResult};
use crate::forms::write::pdf_text;
use crate::properties::{decode_text_string, parse_pdf_date};

/// Marge ajoutée à la taille mesurée de la signature CMS pour réserver `/Contents` (une
/// signature ECDSA varie de quelques octets d'un essai à l'autre).
const CONTENTS_MARGIN: usize = 512;

/// Espace réservé d'une valeur de `/ByteRange` : dix chiffres, soit des fichiers jusqu'à
/// ~9 Go (au-delà de la limite de 4 Gio de `lopdf`).
const PLACEHOLDER: i64 = 9_999_999_999;

/// Plafond de nœuds référencés visités dans l'arbre des champs : un fichier hostile ne peut
/// pas faire durer la lecture indéfiniment.
const MAX_FIELD_NODES: usize = 10_000;

/// Plafond de signatures vérifiées dans un fichier (chacune hache presque tout le fichier).
const MAX_SIGNATURES: usize = 64;

/// Informations facultatives écrites dans le dictionnaire de signature.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignOptions {
    /// Motif (`/Reason`).
    pub reason: Option<String>,
    /// Lieu (`/Location`).
    pub location: Option<String>,
    /// Moyen de contacter le signataire (`/ContactInfo`).
    pub contact: Option<String>,
}

/// Une signature trouvée dans un document, vérifiée.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignatureInfo {
    /// Nom complet du champ de signature.
    pub field: String,
    /// Nom commun du certificat signataire (déclaré par lui, non certifié).
    pub signer: Option<String>,
    /// Date de signature (secondes Unix) : attribut CMS `signing-time`, sinon `/M`.
    pub signed_at: Option<i64>,
    /// Motif déclaré.
    pub reason: Option<String>,
    /// Lieu déclaré.
    pub location: Option<String>,
    /// Format de la signature (`/SubFilter`, par exemple `adbe.pkcs7.detached`).
    pub sub_filter: String,
    /// Verdict.
    pub status: SignatureStatus,
    /// Vrai si la signature couvre tout le fichier ; faux si des modifications ont été
    /// ajoutées après elle (révision antérieure).
    pub covers_whole_document: bool,
}

fn refused(message: &str) -> CoreError {
    CoreError::InvalidOperation(message.to_string())
}

fn structure(e: impl std::fmt::Display) -> CoreError {
    CoreError::InvalidOperation(format!("structure du PDF illisible : {e}"))
}

/// Date PDF `D:AAAAMMJJHHmmSS+00'00'` (§7.9.4) d'un instant Unix, en UTC.
fn pdf_date(unix_seconds: i64) -> String {
    chrono::DateTime::from_timestamp(unix_seconds, 0)
        .map(|d| d.format("D:%Y%m%d%H%M%S+00'00'").to_string())
        .unwrap_or_else(|| "D:19700101000000+00'00'".to_string())
}

/// Texte d'un objet chaîne PDF (PDFDocEncoding ou UTF-16BE avec BOM).
fn text_of(object: Option<&Object>) -> Option<String> {
    match object? {
        Object::String(bytes, _) => Some(decode_text_string(bytes)),
        _ => None,
    }
}

/// Suit une référence (une seule indirection suffit pour les objets cherchés ici).
fn resolve<'a>(doc: &'a LoDocument, object: &'a Object) -> Option<&'a Object> {
    match object {
        Object::Reference(id) => doc.get_object(*id).ok(),
        other => Some(other),
    }
}

fn dict<'a>(doc: &'a LoDocument, object: &'a Object) -> Option<&'a Dictionary> {
    resolve(doc, object)?.as_dict().ok()
}

/// Le dictionnaire `/AcroForm` du catalogue, s'il existe.
fn acroform(doc: &LoDocument) -> Option<&Dictionary> {
    let catalog = doc.catalog().ok()?;
    dict(doc, catalog.get(b"AcroForm").ok()?)
}

/// Un champ de signature trouvé : son nom complet et son dictionnaire de valeur `/V`.
struct SigField<'a> {
    name: String,
    value: &'a Dictionary,
    /// Position dans le fichier de l'objet qui contient ce dictionnaire (lui-même s'il est
    /// référencé, sinon le champ) ; `None` si elle est inconnue (flux d'objets).
    holder_offset: Option<usize>,
}

/// Position dans le fichier de la définition **la plus récente** de l'objet `id`.
fn object_offset(doc: &LoDocument, id: ObjectId) -> Option<usize> {
    match doc.reference_table.get(id.0)? {
        XrefEntry::Normal { offset, generation } if *generation == id.1 => {
            usize::try_from(*offset).ok()
        }
        _ => None,
    }
}

/// Parcourt l'arbre des champs (`/Fields`, `/Kids`) et retient les champs de signature
/// (`/FT /Sig`, héritable) qui portent une valeur.
fn signature_fields(doc: &LoDocument) -> Vec<SigField<'_>> {
    fn walk<'a>(
        doc: &'a LoDocument,
        object: &'a Object,
        parent_name: &str,
        parent_ft: Option<&'a [u8]>,
        depth: usize,
        seen: &mut HashSet<ObjectId>,
        out: &mut Vec<SigField<'a>>,
    ) {
        if depth > 32 || seen.len() > MAX_FIELD_NODES || out.len() >= MAX_SIGNATURES {
            return;
        }
        // Un nœud référencé n'est visité qu'une fois : un `/Kids` qui se cite lui-même, ou un
        // champ cité deux fois, ne fait pas boucler la lecture.
        let node_id = match object {
            Object::Reference(id) => {
                if !seen.insert(*id) {
                    return;
                }
                Some(*id)
            }
            _ => None,
        };
        let Some(node) = dict(doc, object) else {
            return;
        };
        let partial = text_of(node.get(b"T").ok()).unwrap_or_default();
        let name = match (parent_name.is_empty(), partial.is_empty()) {
            (_, true) => parent_name.to_string(),
            (true, false) => partial,
            (false, false) => format!("{parent_name}.{partial}"),
        };
        let ft = node
            .get(b"FT")
            .ok()
            .and_then(|o| o.as_name().ok())
            .or(parent_ft);
        if ft == Some(b"Sig".as_slice())
            && let Ok(v) = node.get(b"V")
            && let Some(value) = dict(doc, v)
        {
            let holder = match v {
                Object::Reference(id) => Some(*id),
                _ => node_id,
            };
            out.push(SigField {
                name: name.clone(),
                value,
                holder_offset: holder.and_then(|id| object_offset(doc, id)),
            });
        }
        if let Ok(kids) = node.get(b"Kids").and_then(Object::as_array) {
            for kid in kids {
                walk(doc, kid, &name, ft, depth + 1, seen, out);
            }
        }
    }
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let Some(form) = acroform(doc) else {
        return out;
    };
    let Some(fields) = form
        .get(b"Fields")
        .ok()
        .and_then(|f| resolve(doc, f))
        .and_then(|f| f.as_array().ok())
    else {
        return out;
    };
    for field in fields {
        walk(doc, field, "", None, 0, &mut seen, &mut out);
    }
    out
}

/// `/ByteRange [0 a b c]` contrôlé : commence à 0, plages dans le fichier, et l'espace exclu
/// est exactement une chaîne hexadécimale `<…>` (la valeur `/Contents`).
fn checked_byte_range(
    value: &Dictionary,
    len: usize,
) -> Result<(usize, usize, usize), &'static str> {
    let range = value
        .get(b"ByteRange")
        .and_then(Object::as_array)
        .map_err(|_| "/ByteRange absent")?;
    let numbers: Vec<i64> = range.iter().filter_map(|o| o.as_i64().ok()).collect();
    let [r0, a, b, c] = numbers[..] else {
        return Err("/ByteRange mal formé");
    };
    let (Ok(a), Ok(b), Ok(c)) = (usize::try_from(a), usize::try_from(b), usize::try_from(c)) else {
        return Err("/ByteRange négatif");
    };
    let end = b.checked_add(c).ok_or("/ByteRange hors du fichier")?;
    if r0 != 0 || a == 0 || b <= a || end > len {
        return Err("/ByteRange hors du fichier");
    }
    Ok((a, b, c))
}

/// Octets de la signature : la chaîne hexadécimale entre `<` (en `a`) et `>` (en `b - 1`).
fn contents_bytes(bytes: &[u8], a: usize, b: usize) -> Result<Vec<u8>, &'static str> {
    if bytes.get(a) != Some(&b'<') || bytes.get(b - 1) != Some(&b'>') {
        return Err("l'espace exclu n'est pas la valeur /Contents");
    }
    let hex: Vec<u8> = bytes[a + 1..b - 1]
        .iter()
        .copied()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    if !hex.len().is_multiple_of(2) || !hex.iter().all(u8::is_ascii_hexdigit) {
        return Err("valeur /Contents illisible");
    }
    let nibble = |c: u8| -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            _ => c - b'A' + 10,
        }
    };
    Ok(hex
        .as_chunks::<2>()
        .0
        .iter()
        .map(|[h, l]| (nibble(*h) << 4) | nibble(*l))
        .collect())
}

/// Empreintes déjà calculées, par plage `(a, b, c)` : deux champs qui désignent la même
/// plage ne font pas hacher deux fois le fichier.
type DigestCache = HashMap<(usize, usize, usize), [u8; 32]>;

/// Vérifie une signature (dictionnaire `/V`) contre les octets du fichier.
fn check_one(bytes: &[u8], field: &SigField<'_>, digests: &mut DigestCache) -> SignatureInfo {
    let value = field.value;
    let sub_filter = value
        .get(b"SubFilter")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .unwrap_or_default();
    let mut info = SignatureInfo {
        field: field.name.clone(),
        signer: text_of(value.get(b"Name").ok()),
        signed_at: text_of(value.get(b"M").ok()).and_then(|m| parse_pdf_date(&m)),
        reason: text_of(value.get(b"Reason").ok()),
        location: text_of(value.get(b"Location").ok()),
        sub_filter: sub_filter.clone(),
        status: SignatureStatus::Unsupported(SigIssue::CmsUnreadable),
        covers_whole_document: false,
    };
    if sub_filter != "adbe.pkcs7.detached" && sub_filter != "ETSI.CAdES.detached" {
        info.status = SignatureStatus::Unsupported(SigIssue::UnsupportedFormat);
        return info;
    }
    let (a, b, c) = match checked_byte_range(value, bytes.len()) {
        Ok(r) => r,
        Err(_) => {
            info.status = SignatureStatus::Invalid(SigIssue::ByteRangeInvalid);
            return info;
        }
    };
    let der = match contents_bytes(bytes, a, b) {
        Ok(der) => der,
        Err(_) => {
            info.status = SignatureStatus::Invalid(SigIssue::ContentsInvalid);
            return info;
        }
    };
    // Le dictionnaire doit faire partie des octets signés (il commence avant le trou qu'il
    // contient). Sinon, une révision ajoutée après la signature l'a redéfini : son nom, son
    // motif, son lieu et sa date `/M` ne sont pas signés et ne sont donc pas affichés.
    if !field.holder_offset.is_some_and(|offset| offset < a) {
        info.signer = None;
        info.signed_at = None;
        info.reason = None;
        info.location = None;
    }
    // Empreinte des deux plages, sans recopier le fichier.
    let digest = *digests.entry((a, b, c)).or_insert_with(|| {
        let mut hasher = Sha256::new();
        hasher.update(&bytes[..a]);
        hasher.update(&bytes[b..b + c]);
        hasher.finalize().into()
    });
    let check = cms::verify(&der, &digest);
    if check.signer.is_some() {
        info.signer = check.signer;
    }
    if check.signed_at.is_some() {
        info.signed_at = check.signed_at;
    }
    info.status = check.status;
    // Couvre tout le fichier si rien ne suit la seconde plage, sinon des blancs seulement.
    info.covers_whole_document = bytes[b + c..].iter().all(u8::is_ascii_whitespace);
    info
}

/// Les signatures de ce fichier PDF, vérifiées (aucune : liste vide). Un PDF chiffré par mot
/// de passe utilisateur se lit avec [`Document::signatures`], qui connaît ce mot de passe.
pub fn signatures(bytes: &[u8]) -> CoreResult<Vec<SignatureInfo>> {
    signatures_with(bytes, None)
}

fn signatures_with(bytes: &[u8], password: Option<&str>) -> CoreResult<Vec<SignatureInfo>> {
    // Toute signature vérifiable a son `/ByteRange` écrit en clair dans le fichier (le trou
    // de `/Contents` doit s'y trouver) : sans ce mot, pas besoin d'analyser tout le PDF, ce
    // qui retarderait l'affichage de chaque document ouvert.
    if !bytes.windows(10).any(|w| w == b"/ByteRange") {
        return Ok(Vec::new());
    }
    let doc = match password {
        Some(p) => LoDocument::load_mem_with_options(bytes, lopdf::LoadOptions::with_password(p)),
        None => LoDocument::load_mem(bytes),
    }
    .map_err(structure)?;
    let mut digests = DigestCache::new();
    Ok(signature_fields(&doc)
        .iter()
        .map(|field| check_one(bytes, field, &mut digests))
        .collect())
}

/// Noms (`/T`) des champs de premier niveau, pour choisir un nom libre.
fn top_level_names(doc: &LoDocument) -> Vec<String> {
    let Some(fields) = acroform(doc)
        .and_then(|f| f.get(b"Fields").ok())
        .and_then(|f| resolve(doc, f))
        .and_then(|f| f.as_array().ok())
    else {
        return Vec::new();
    };
    fields
        .iter()
        .filter_map(|f| dict(doc, f))
        .filter_map(|d| text_of(d.get(b"T").ok()))
        .collect()
}

/// Refus propres à la signature : document chiffré, formulaire XFA, document certifié qui
/// n'autorise pas l'ajout d'un champ de signature (`/DocMDP` avec `P = 1` ou `P = 2`).
///
/// `P = 2` permet de « remplir les formulaires et signer », mais seulement dans des champs
/// de signature **existants** : Acrobat juge l'ajout d'un nouveau champ comme une
/// modification interdite, qui casserait la certification. Signer un champ vide existant
/// n'est pas encore pris en charge, d'où le refus.
fn refuse_unsignable(doc: &LoDocument) -> CoreResult<()> {
    if doc.is_encrypted() || doc.was_encrypted() {
        return Err(refused(
            "signature d'un PDF chiffré : non prise en charge dans cette version",
        ));
    }
    if acroform(doc).is_some_and(|f| f.has(b"XFA")) {
        return Err(refused(
            "formulaire XFA : un lecteur XFA ignorerait la signature, elle est refusée",
        ));
    }
    let certified_locked = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"Perms").ok())
        .and_then(|p| dict(doc, p))
        .and_then(|p| p.get(b"DocMDP").ok())
        .and_then(|s| dict(doc, s))
        .and_then(|s| s.get(b"Reference").ok())
        .and_then(|r| resolve(doc, r))
        .and_then(|r| r.as_array().ok())
        .is_some_and(|refs| {
            refs.iter().filter_map(|r| dict(doc, r)).any(|r| {
                r.get(b"TransformParams")
                    .ok()
                    .and_then(|t| dict(doc, t))
                    .and_then(|t| t.get(b"P").ok())
                    .and_then(|p| p.as_i64().ok())
                    .is_some_and(|p| p == 1 || p == 2)
            })
        });
    if certified_locked {
        return Err(refused(
            "document certifié : y ajouter un champ de signature casserait sa certification",
        ));
    }
    Ok(())
}

/// Ajoute `item` au tableau `key` de l'objet `owner` de la nouvelle révision (tableau direct
/// ou référencé ; créé s'il manque). Les objets anciens sont d'abord recopiés dans la
/// nouvelle révision, la seule que l'on modifie.
fn push_to_array(
    inc: &mut IncrementalDocument,
    owner: ObjectId,
    key: &[u8],
    item: Object,
) -> CoreResult<()> {
    inc.opt_clone_object_to_new_document(owner)
        .map_err(structure)?;
    let existing = inc
        .new_document
        .get_dictionary(owner)
        .map_err(structure)?
        .get(key)
        .ok()
        .cloned();
    match existing {
        Some(Object::Reference(array_id)) => {
            inc.opt_clone_object_to_new_document(array_id)
                .map_err(structure)?;
            match inc
                .new_document
                .get_object_mut(array_id)
                .map_err(structure)?
            {
                Object::Array(items) => items.push(item),
                _ => return Err(structure("tableau attendu")),
            }
        }
        Some(Object::Array(mut items)) => {
            items.push(item);
            inc.new_document
                .get_dictionary_mut(owner)
                .map_err(structure)?
                .set(key, Object::Array(items));
        }
        _ => {
            inc.new_document
                .get_dictionary_mut(owner)
                .map_err(structure)?
                .set(key, Object::Array(vec![item]));
        }
    }
    Ok(())
}

/// Position unique de `needle` dans `haystack` à partir de `from` ; `None` si absente ou
/// présente plusieurs fois (un espace réservé ambigu ne doit jamais être réécrit).
fn find_unique(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    let mut found = None;
    let mut start = from;
    while let Some(pos) = haystack
        .get(start..)?
        .windows(needle.len())
        .position(|w| w == needle)
    {
        if found.is_some() {
            return None;
        }
        found = Some(start + pos);
        start += pos + 1;
    }
    found
}

/// Signe `base` (un PDF lisible par `lopdf`, non chiffré) : retourne le fichier complet, la
/// signature ajoutée par mise à jour incrémentale.
pub(crate) fn sign_bytes(
    base: &[u8],
    identity: &SigningIdentity,
    options: &SignOptions,
    now: i64,
) -> CoreResult<Vec<u8>> {
    let prev = LoDocument::load_mem(base).map_err(structure)?;
    refuse_unsignable(&prev)?;
    let root_id = prev
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(structure)?;
    let first_page = *prev
        .get_pages()
        .get(&1)
        .ok_or_else(|| structure("document sans page"))?;
    let names = top_level_names(&prev);
    let field_name = (1..)
        .map(|n| format!("Signature{n}"))
        .find(|n| !names.contains(n))
        .unwrap_or_else(|| "Signature".to_string());
    let acroform_ref = prev
        .catalog()
        .ok()
        .and_then(|c| c.get(b"AcroForm").ok())
        .cloned();

    // Taille réservée : un essai à blanc donne la longueur de la structure CMS.
    let reserved = cms::sign_digest(identity, &[0u8; 32], now)?.len() + CONTENTS_MARGIN;

    let mut inc = IncrementalDocument::create_from(base.to_vec(), prev);
    let mut sig = Dictionary::new();
    sig.set("Type", Object::Name(b"Sig".to_vec()));
    sig.set("Filter", Object::Name(b"Adobe.PPKLite".to_vec()));
    sig.set("SubFilter", Object::Name(b"adbe.pkcs7.detached".to_vec()));
    sig.set(
        "ByteRange",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(PLACEHOLDER),
            Object::Integer(PLACEHOLDER),
            Object::Integer(PLACEHOLDER),
        ]),
    );
    sig.set(
        "Contents",
        Object::String(vec![0u8; reserved], StringFormat::Hexadecimal),
    );
    sig.set("M", pdf_text(&pdf_date(now)));
    sig.set("Name", pdf_text(&identity.signer_name()));
    for (key, value) in [
        ("Reason", &options.reason),
        ("Location", &options.location),
        ("ContactInfo", &options.contact),
    ] {
        if let Some(v) = value.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            sig.set(key, pdf_text(v));
        }
    }
    let sig_id = inc.new_document.add_object(Object::Dictionary(sig));

    // Champ de signature invisible, fusionné avec son widget, sur la première page.
    let widget_id = inc
        .new_document
        .add_object(Object::Dictionary(Dictionary::from_iter(vec![
            ("Type", Object::Name(b"Annot".to_vec())),
            ("Subtype", Object::Name(b"Widget".to_vec())),
            ("FT", Object::Name(b"Sig".to_vec())),
            ("T", pdf_text(&field_name)),
            ("V", Object::Reference(sig_id)),
            // Imprimable (4) + verrouillée (128), jamais cachée.
            ("F", Object::Integer(132)),
            ("Rect", Object::Array(vec![Object::Integer(0); 4])),
            ("P", Object::Reference(first_page)),
        ])));
    push_to_array(
        &mut inc,
        first_page,
        b"Annots",
        Object::Reference(widget_id),
    )?;

    // `/AcroForm` : référencé (on recopie cet objet), direct (on recopie le catalogue) ou
    // absent (on le crée). `/SigFlags 3` : signatures présentes, ajout seulement.
    let form_owner = match acroform_ref {
        Some(Object::Reference(form_id)) => form_id,
        Some(Object::Dictionary(direct)) => {
            let form_id = inc.new_document.add_object(Object::Dictionary(direct));
            inc.opt_clone_object_to_new_document(root_id)
                .map_err(structure)?;
            inc.new_document
                .get_dictionary_mut(root_id)
                .map_err(structure)?
                .set("AcroForm", Object::Reference(form_id));
            form_id
        }
        _ => {
            let form_id = inc
                .new_document
                .add_object(Object::Dictionary(Dictionary::new()));
            inc.opt_clone_object_to_new_document(root_id)
                .map_err(structure)?;
            inc.new_document
                .get_dictionary_mut(root_id)
                .map_err(structure)?
                .set("AcroForm", Object::Reference(form_id));
            form_id
        }
    };
    push_to_array(
        &mut inc,
        form_owner,
        b"Fields",
        Object::Reference(widget_id),
    )?;
    inc.new_document
        .get_dictionary_mut(form_owner)
        .map_err(structure)?
        .set("SigFlags", Object::Integer(3));

    let mut out = Vec::new();
    inc.save_to(&mut out).map_err(structure)?;

    // Espaces réservés : cherchés dans la seule partie ajoutée, et uniques.
    let tail = base.len();
    let range_hole = format!("{PLACEHOLDER} {PLACEHOLDER} {PLACEHOLDER}]");
    let range_end = find_unique(&out, range_hole.as_bytes(), tail)
        .ok_or_else(|| structure("espace réservé /ByteRange introuvable"))?
        + range_hole.len();
    let range_start = out[..range_end]
        .iter()
        .rposition(|&c| c == b'[')
        .ok_or_else(|| structure("espace réservé /ByteRange introuvable"))?;
    let mut contents_hole = Vec::with_capacity(reserved * 2 + 2);
    contents_hole.push(b'<');
    contents_hole.resize(reserved * 2 + 1, b'0');
    contents_hole.push(b'>');
    let a = find_unique(&out, &contents_hole, tail)
        .ok_or_else(|| structure("espace réservé /Contents introuvable"))?;
    let b = a + contents_hole.len();
    let c = out.len() - b;

    // `/ByteRange` réel, à la même largeur (complété d'espaces avant le `]`).
    let width = range_end - range_start;
    let mut range = format!("[0 {a} {b} {c}");
    if range.len() + 1 > width {
        return Err(structure(
            "fichier trop grand pour l'espace réservé /ByteRange",
        ));
    }
    while range.len() + 1 < width {
        range.push(' ');
    }
    range.push(']');
    out[range_start..range_end].copy_from_slice(range.as_bytes());

    // Signature des deux plages, recopiée en hexadécimal dans l'espace réservé.
    let mut signed = Vec::with_capacity(a + c);
    signed.extend_from_slice(&out[..a]);
    signed.extend_from_slice(&out[b..]);
    let der = cms::sign_digest(identity, &sha256(&signed), now)?;
    if der.len() > reserved {
        return Err(structure("signature plus grande que l'espace réservé"));
    }
    let hex: String = der.iter().map(|byte| format!("{byte:02X}")).collect();
    out[a + 1..a + 1 + hex.len()].copy_from_slice(hex.as_bytes());
    Ok(out)
}

impl Document {
    /// Les signatures du **fichier** (tel que chargé ou enregistré), vérifiées : une
    /// modification en cours n'y change rien tant qu'elle n'est pas enregistrée.
    pub fn signatures(&self) -> CoreResult<Vec<SignatureInfo>> {
        signatures_with(self.file_bytes(), self.password())
    }

    /// Signe le document avec `identity` et l'enregistre dans `path` : la signature est
    /// ajoutée par mise à jour incrémentale (les signatures existantes restent valides), le
    /// résultat est revérifié avant d'être écrit, puis le document est rechargé depuis ces
    /// octets. Un document modifié est d'abord réécrit comme par « Enregistrer sous ».
    pub fn sign_to(
        &mut self,
        path: impl AsRef<Path>,
        identity: &SigningIdentity,
        options: &SignOptions,
    ) -> CoreResult<()> {
        let path = path.as_ref();
        if self.password().is_some() {
            return Err(refused(
                "signature d'un PDF chiffré : non prise en charge dans cette version",
            ));
        }
        let mut base = if self.changed {
            self.rewritten_bytes()?
        } else {
            self.file_bytes().to_vec()
        };
        // Table de références reconstruite par `lopdf` : une mise à jour incrémentale serait
        // sans `/Prev`. On réécrit d'abord le fichier, sauf s'il porte déjà des signatures
        // (elles seraient invalidées).
        let needs_rewrite = match LoDocument::load_mem(&base) {
            Ok(doc) => doc.xref_recovery.is_some(),
            Err(_) => true,
        };
        if needs_rewrite {
            if signatures(&base).is_ok_and(|s| !s.is_empty()) {
                return Err(refused(
                    "structure du fichier abîmée et déjà signée : l'ajout d'une signature invaliderait les précédentes",
                ));
            }
            base = self.rewritten_bytes()?;
        }
        let now = chrono::Utc::now().timestamp();
        let signed = sign_bytes(&base, identity, options, now)?;
        // Contrôle avant écriture : la signature ajoutée est intègre et couvre tout.
        let ok = signatures(&signed)?
            .iter()
            .any(|s| s.status == SignatureStatus::Intact && s.covers_whole_document);
        if !ok {
            return Err(structure("la signature produite ne se vérifie pas"));
        }
        write_atomically(path, &signed)?;
        self.reload_from_bytes(signed.clone())?;
        self.mark_saved(path, signed);
        Ok(())
    }
}
