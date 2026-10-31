// SPDX-License-Identifier: GPL-3.0-or-later
//! Sous-ensemble de la police Unicode que PDFium incorpore **entière** (Lot 14, audit E10).
//!
//! Pour dessiner un texte hors WinAnsi (insertion de texte, texte libre, tampon), PDFium
//! charge notre DejaVu Sans complète (`Document::unicode_font_unlocked`) et l'écrit telle
//! quelle : environ 386 Ko comprimés, pour quelques lettres. La passe `lopdf`
//! (`fonts::unicode::Type0Writer`), elle, n'incorpore que les glyphes utiles. Cette passe
//! d'enregistrement ramène la police de PDFium au même régime :
//!
//! 1. repérer les descripteurs dont `/FontFile2` est exactement la DejaVu embarquée ;
//! 2. relever, dans les contenus de page, les formulaires (`/XObject`) et les apparences
//!    d'annotation (`/AP`), les glyphes que dessinent les polices `Type0` qui s'en servent ;
//! 3. réduire le programme à ces glyphes (`subsetter`) et réécrire la table `/CIDToGIDMap`.
//!
//! Pourquoi réécrire `/CIDToGIDMap` plutôt que les chaînes : `subsetter` **renumérote** les
//! glyphes gardés (1, 2, 3…), il ne sait pas garder leurs numéros d'origine. Les chaînes de
//! texte de PDFium donnent des CID égaux aux numéros de glyphe d'origine (Identity) ; on
//! garde ces CID, et la table dit, pour chacun, le nouveau glyphe. `/W`, `/DW` et
//! `/ToUnicode` sont indexés par CID : ils restent justes sans y toucher, et aucun flux de
//! contenu n'est modifié (la relecture de contrôle de `before_save_with` les compare).
//!
//! Prudence : une police n'est réduite que si **tous** ses usages sont connus. Une police
//! citée ailleurs que dans les ressources parcourues (formulaire `/DR`, police Type 3…), un
//! contenu illisible, trop de glyphes ou des formulaires trop imbriqués : la police (ou la
//! passe entière) reste intacte. Jamais une police cassée, au pire une police trop grosse.

use std::collections::{BTreeMap, BTreeSet};

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::error::CoreResult;
use crate::fonts::unicode::{DEJAVU_SANS, compressed, is_full_dejavu, subset_tag};

/// Nombre maximal de glyphes relevés dans les chaînes de texte (au-delà, la passe renonce) :
/// borne le travail sur un fichier hostile, bien au-dessus d'un document ordinaire.
const MAX_GLYPH_REFS: usize = 100_000;

/// Profondeur maximale des formulaires imbriqués (`/XObject` dans `/XObject`) parcourus.
const MAX_FORM_DEPTH: usize = 8;

/// Nombre maximal de contenus parcourus (pages, formulaires, apparences) : au-delà, la passe
/// renonce plutôt que de s'épuiser sur un fichier construit pour cela.
const MAX_CONTENTS: usize = 100_000;

/// Une police descendante (`CIDFontType2`) dont le descripteur porte la DejaVu entière.
#[derive(Debug)]
struct CidFont {
    /// Son `/FontDescriptor`.
    descriptor: ObjectId,
    /// Table CID → glyphe d'origine ; `None` pour `/Identity` (ou table absente) : le
    /// glyphe est le CID lui-même. C'est ce qu'écrit PDFium.
    map: Option<Vec<u16>>,
}

/// Les polices dont le programme est la DejaVu entière, et leurs liens.
#[derive(Debug, Default)]
struct Candidates {
    /// Programme (flux `/FontFile2`) → descripteurs qui le citent (après `dedupe_font_files`,
    /// plusieurs descripteurs partagent souvent un même programme).
    files: BTreeMap<ObjectId, Vec<ObjectId>>,
    /// Descripteur → programme.
    descriptors: BTreeMap<ObjectId, ObjectId>,
    /// Polices descendantes de ces descripteurs.
    cid_fonts: BTreeMap<ObjectId, CidFont>,
    /// Police `Type0` → sa police descendante.
    type0: BTreeMap<ObjectId, ObjectId>,
    /// Programmes à laisser intacts (usage inconnu ou table illisible).
    untouchable: BTreeSet<ObjectId>,
}

impl Candidates {
    /// Le programme dessiné par la police descendante `cid_font`.
    fn file_of(&self, cid_font: ObjectId) -> Option<ObjectId> {
        let font = self.cid_fonts.get(&cid_font)?;
        self.descriptors.get(&font.descriptor).copied()
    }
}

/// Ce que le parcours des contenus a relevé.
#[derive(Debug, Default)]
struct Usage {
    /// Programme → glyphes d'origine dessinés.
    glyphs: BTreeMap<ObjectId, BTreeSet<u16>>,
    /// Objets qui portent des ressources parcourues (page, flux, `/Resources` ou `/Font`
    /// indirects) : les seuls autorisés à citer une police `Type0` candidate.
    holders: BTreeSet<ObjectId>,
    /// Glyphes relevés jusqu'ici (borne [`MAX_GLYPH_REFS`]).
    glyph_refs: usize,
}

/// Remplace chaque programme DejaVu **entier** incorporé par PDFium par un sous-ensemble des
/// glyphes réellement dessinés, avec `/CIDToGIDMap`, `/CIDSet` et une étiquette
/// `XXXXXX+DejaVuSans` (§9.6.4). Appelée par `before_save_with` après `dedupe_font_files`,
/// pour qu'un programme partagé donne **un** sous-ensemble partagé.
///
/// Pourquoi : un fichier de 2 Ko où l'on insère « Ω » en pesait 420 (audit E10). Une police
/// dont un usage échappe au parcours reste entière (voir l'en-tête du module) ; une police
/// que plus rien ne dessine est laissée à l'élagage.
pub(crate) fn subset_embedded_fonts(doc: &mut Document) -> CoreResult<()> {
    let Some(mut candidates) = find_candidates(doc) else {
        return Ok(());
    };
    // Un contenu illisible ou trop gros : on ne sait pas tout ce qui est dessiné, rien ne
    // change.
    let Some(usage) = collect_usage(doc, &candidates) else {
        return Ok(());
    };
    mark_foreign_references(doc, &mut candidates, &usage);
    let files: Vec<ObjectId> = candidates.files.keys().copied().collect();
    for file in files {
        if candidates.untouchable.contains(&file) {
            continue;
        }
        let Some(glyphs) = usage.glyphs.get(&file).filter(|g| !g.is_empty()) else {
            continue;
        };
        if let Some(plan) = plan_subset(&candidates, file, glyphs) {
            apply_plan(doc, &candidates, file, plan);
        }
    }
    Ok(())
}

/// Les polices bâties sur la DejaVu entière ; `None` s'il n'y en a aucune (cas courant :
/// l'enregistrement ne paie alors qu'un passage sur les objets).
fn find_candidates(doc: &Document) -> Option<Candidates> {
    let mut c = Candidates::default();
    // Programme déjà examiné → est-ce la DejaVu entière ?
    let mut checked: BTreeMap<ObjectId, bool> = BTreeMap::new();
    for (id, object) in &doc.objects {
        let Ok(dict) = object.as_dict() else {
            continue;
        };
        if name_of(dict, b"Type") != Some(b"FontDescriptor") {
            continue;
        }
        let Ok(file) = dict.get(b"FontFile2").and_then(Object::as_reference) else {
            continue;
        };
        let full = *checked
            .entry(file)
            .or_insert_with(|| is_full_program(doc, file));
        if full {
            c.files.entry(file).or_default().push(*id);
            c.descriptors.insert(*id, file);
        }
    }
    if c.files.is_empty() {
        return None;
    }
    for (id, object) in &doc.objects {
        let Ok(dict) = object.as_dict() else {
            continue;
        };
        if name_of(dict, b"Subtype") != Some(b"CIDFontType2") {
            continue;
        }
        let Ok(descriptor) = dict.get(b"FontDescriptor").and_then(Object::as_reference) else {
            continue;
        };
        let Some(file) = c.descriptors.get(&descriptor).copied() else {
            continue;
        };
        match cid_to_gid(doc, dict) {
            Some(map) => {
                c.cid_fonts.insert(*id, CidFont { descriptor, map });
            }
            None => {
                c.untouchable.insert(file);
            }
        }
    }
    for (id, object) in &doc.objects {
        let Ok(dict) = object.as_dict() else {
            continue;
        };
        if name_of(dict, b"Subtype") != Some(b"Type0") {
            continue;
        }
        let descendant = dict
            .get(b"DescendantFonts")
            .ok()
            .and_then(|d| doc.dereference(d).ok())
            .and_then(|(_, d)| d.as_array().ok()?.first()?.as_reference().ok());
        if let Some(cid_font) = descendant.filter(|d| c.cid_fonts.contains_key(d)) {
            c.type0.insert(*id, cid_font);
        }
    }
    Some(c)
}

/// Vrai si le flux `file` contient exactement la DejaVu embarquée. `/Length1` (taille
/// décomprimée déclarée) écarte d'abord, sans rien décomprimer, les autres polices.
fn is_full_program(doc: &Document, file: ObjectId) -> bool {
    let Ok(stream) = doc.get_object(file).and_then(Object::as_stream) else {
        return false;
    };
    if let Ok(length1) = stream.dict.get(b"Length1").and_then(Object::as_i64)
        && usize::try_from(length1).ok() != Some(DEJAVU_SANS.len())
    {
        return false;
    }
    // Plafond : un octet de plus que la DejaVu suffit à conclure, sans décomprimer au-delà.
    crate::crypt::stream_bytes_within(stream, DEJAVU_SANS.len() + 1)
        .is_some_and(|bytes| is_full_dejavu(&bytes))
}

/// La table `/CIDToGIDMap` d'une police descendante : `Some(None)` pour l'identité (nom
/// `/Identity` ou clé absente, §9.7.4.2), `Some(Some(table))` pour un flux (deux octets
/// gros-boutistes par CID), `None` si elle est illisible.
fn cid_to_gid(doc: &Document, cid_font: &Dictionary) -> Option<Option<Vec<u16>>> {
    let Ok(map) = cid_font.get(b"CIDToGIDMap") else {
        return Some(None);
    };
    match doc.dereference(map).ok()?.1 {
        Object::Name(name) if name == b"Identity" => Some(None),
        Object::Stream(stream) => {
            // 65 536 CID au plus, deux octets chacun.
            let bytes = crate::crypt::stream_bytes_within(stream, 2 * 65_536)?;
            Some(Some(
                bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_be_bytes(*pair))
                    .collect(),
            ))
        }
        _ => None,
    }
}

/// D'où viennent les octets d'un contenu à parcourir.
#[derive(Debug, Clone, Copy)]
enum Source {
    /// Le contenu (éventuellement en plusieurs flux) d'une page.
    Page(ObjectId),
    /// Un flux : formulaire, apparence d'annotation, glyphe Type 3, motif.
    Stream(ObjectId),
}

/// Un contenu en attente de parcours, avec les ressources où chercher ses noms de police.
struct Work {
    source: Source,
    resources: Dictionary,
    /// L'objet qui fournit `resources` : la page, ou le flux qui a ses propres ressources.
    /// Un formulaire **sans** ressources prend celles de qui le dessine : le même
    /// formulaire, dessiné depuis deux pages, est alors parcouru deux fois, une par jeu de
    /// ressources (les mêmes noms peuvent y désigner deux polices différentes).
    owner: ObjectId,
    depth: usize,
}

/// Relève les glyphes dessinés par les polices candidates dans tout le document. `None` si un
/// contenu ne se lit pas ou si une borne est franchie : on ne sait alors pas tout ce qui est
/// dessiné.
///
/// \[Apprentissage\] **Pile explicite** plutôt que récursion : un formulaire peut contenir un
/// formulaire, qui en contient un autre… Une fonction qui s'appellerait elle-même pour chacun
/// laisserait un fichier hostile choisir la profondeur de la pile d'appels (et la faire
/// déborder). Ici, `todo` est un simple `Vec` : on y pousse les contenus découverts et on les
/// traite un par un, avec une profondeur comptée et un ensemble `visited` contre les cycles.
fn collect_usage(doc: &Document, c: &Candidates) -> Option<Usage> {
    let mut usage = Usage::default();
    let mut todo: Vec<Work> = Vec::new();
    // (flux, fournisseur de ses ressources) déjà mis en file : contre les cycles et les
    // doublons.
    let mut visited: BTreeSet<(ObjectId, ObjectId)> = BTreeSet::new();
    for page_id in doc.get_pages().into_values() {
        let (resources, holders) = page_resources(doc, page_id);
        usage.holders.extend(holders);
        todo.push(Work {
            source: Source::Page(page_id),
            resources,
            owner: page_id,
            depth: 0,
        });
        for stream in appearance_streams(doc, page_id) {
            if visited.insert((stream, stream)) {
                let resources = own_resources(doc, stream, &mut usage.holders);
                todo.push(Work {
                    source: Source::Stream(stream),
                    resources: resources.unwrap_or_default(),
                    owner: stream,
                    depth: 1,
                });
            }
        }
    }
    let mut walked = 0;
    while let Some(work) = todo.pop() {
        walked += 1;
        if walked > MAX_CONTENTS {
            return None;
        }
        let bytes = match work.source {
            Source::Page(id) => crate::crypt::page_content(doc, id).ok()?,
            Source::Stream(id) => {
                crate::crypt::stream_bytes(doc.get_object(id).and_then(Object::as_stream).ok()?)?
            }
        };
        // Lecture **stricte** : `Content::decode` s'arrête en silence au premier octet
        // illisible et ignore la suite, que PDFium, lui, dessinerait peut-être ; des glyphes
        // y manqueraient au sous-ensemble.
        let content = Content::decode_strict(&bytes).ok()?;
        let fonts = font_map(doc, &work.resources, &mut usage.holders);
        read_strings(c, &fonts, &content, &mut usage)?;
        // Formulaires, motifs et glyphes Type 3 cités par ces ressources : tous parcourus,
        // qu'ils soient dessinés ou non (relever trop de glyphes ne casse rien).
        for child in nested_streams(doc, &work.resources, &fonts) {
            let own = own_resources(doc, child.stream, &mut usage.holders).or(child.resources);
            let owner = if own.is_some() {
                child.stream
            } else {
                work.owner
            };
            if !visited.insert((child.stream, owner)) {
                continue;
            }
            if work.depth >= MAX_FORM_DEPTH {
                return None;
            }
            todo.push(Work {
                source: Source::Stream(child.stream),
                resources: own.unwrap_or_else(|| work.resources.clone()),
                owner,
                depth: work.depth + 1,
            });
        }
    }
    Some(usage)
}

/// Les ressources d'une page, héritées de ses ancêtres au besoin (comme
/// `overlay::inherited_resources`), avec les objets qui les portent : la page ou l'ancêtre,
/// et le dictionnaire `/Resources` s'il est indirect.
fn page_resources(doc: &Document, page_id: ObjectId) -> (Dictionary, Vec<ObjectId>) {
    let mut node_id = page_id;
    for _ in 0..32 {
        let Ok(node) = doc.get_dictionary(node_id) else {
            break;
        };
        if let Ok(res) = node.get(b"Resources")
            && let Ok((_, Object::Dictionary(dict))) = doc.dereference(res)
        {
            let mut holders = vec![node_id];
            holders.extend(res.as_reference().ok());
            return (dict.clone(), holders);
        }
        let Ok(parent) = node.get(b"Parent").and_then(Object::as_reference) else {
            break;
        };
        node_id = parent;
    }
    (Dictionary::new(), Vec::new())
}

/// Les `/Resources` propres du flux `stream`, s'il en a ; le flux (et le dictionnaire
/// indirect) rejoignent les porteurs de ressources parcourues.
fn own_resources(
    doc: &Document,
    stream: ObjectId,
    holders: &mut BTreeSet<ObjectId>,
) -> Option<Dictionary> {
    let dict = &doc
        .get_object(stream)
        .and_then(Object::as_stream)
        .ok()?
        .dict;
    let res = dict.get(b"Resources").ok()?;
    let (_, Object::Dictionary(resources)) = doc.dereference(res).ok()? else {
        return None;
    };
    holders.insert(stream);
    holders.extend(res.as_reference().ok());
    Some(resources.clone())
}

/// Les polices des ressources (nom → objet), sous-dictionnaire `/Font` indirect compris.
fn font_map(
    doc: &Document,
    resources: &Dictionary,
    holders: &mut BTreeSet<ObjectId>,
) -> Dictionary {
    let Ok(fonts) = resources.get(b"Font") else {
        return Dictionary::new();
    };
    match doc.dereference(fonts) {
        Ok((_, Object::Dictionary(dict))) => {
            holders.extend(fonts.as_reference().ok());
            dict.clone()
        }
        _ => Dictionary::new(),
    }
}

/// Les flux d'apparence (`/AP` : `/N`, `/D`, `/R`, ou leurs sous-dictionnaires d'états) des
/// annotations de la page (tableau `/Annots` direct ou indirect, annotations en ligne ou
/// citées : PDFium écrit les fraîches en ligne).
fn appearance_streams(doc: &Document, page_id: ObjectId) -> Vec<ObjectId> {
    let mut out = Vec::new();
    let Some(annots) = doc
        .get_dictionary(page_id)
        .ok()
        .and_then(|page| page.get(b"Annots").ok())
        .and_then(|a| doc.dereference(a).ok())
        .and_then(|(_, a)| a.as_array().ok())
    else {
        return out;
    };
    for annot in annots {
        let Some(ap) = doc
            .dereference(annot)
            .ok()
            .and_then(|(_, a)| a.as_dict().ok())
            .and_then(|a| a.get(b"AP").ok())
            .and_then(|ap| doc.dereference(ap).ok())
            .and_then(|(_, ap)| ap.as_dict().ok())
        else {
            continue;
        };
        for key in [b"N".as_slice(), b"D", b"R"] {
            let Ok(entry) = ap.get(key) else {
                continue;
            };
            if let Some(id) = stream_ref(doc, entry) {
                out.push(id);
                continue;
            }
            if let Ok((_, Object::Dictionary(states))) = doc.dereference(entry) {
                out.extend(states.iter().filter_map(|(_, s)| stream_ref(doc, s)));
            }
        }
    }
    out
}

/// Le numéro du flux que cite `object` ; `None` pour autre chose (une référence pendante ne
/// dessine rien : l'ignorer est sans risque, alors que la lire ferait renoncer à la passe).
fn stream_ref(doc: &Document, object: &Object) -> Option<ObjectId> {
    let id = object.as_reference().ok()?;
    doc.get_object(id).and_then(Object::as_stream).ok()?;
    Some(id)
}

/// Un flux imbriqué à parcourir, avec les ressources de son parent quand il n'en a pas.
struct Child {
    stream: ObjectId,
    /// Ressources à utiliser si le flux n'en a pas (celles d'une police Type 3).
    resources: Option<Dictionary>,
}

/// Formulaires (`/XObject /Subtype /Form`), motifs en pavage (`/Pattern`, flux) et glyphes
/// des polices Type 3 (`/CharProcs`) cités par des ressources.
fn nested_streams(doc: &Document, resources: &Dictionary, fonts: &Dictionary) -> Vec<Child> {
    let mut out = Vec::new();
    for key in [b"XObject".as_slice(), b"Pattern"] {
        let Some(entries) = resources
            .get(key)
            .ok()
            .and_then(|e| doc.dereference(e).ok())
            .and_then(|(_, e)| e.as_dict().ok())
        else {
            continue;
        };
        for (_, value) in entries.iter() {
            let Ok(id) = value.as_reference() else {
                continue;
            };
            let Ok(stream) = doc.get_object(id).and_then(Object::as_stream) else {
                continue;
            };
            // Une image n'a pas de contenu à lire.
            if key == b"XObject" && name_of(&stream.dict, b"Subtype") != Some(b"Form") {
                continue;
            }
            out.push(Child {
                stream: id,
                resources: None,
            });
        }
    }
    for (_, font) in fonts.iter() {
        let Some(font) = doc
            .dereference(font)
            .ok()
            .and_then(|(_, f)| f.as_dict().ok())
        else {
            continue;
        };
        if name_of(font, b"Subtype") != Some(b"Type3") {
            continue;
        }
        let own = font
            .get(b"Resources")
            .ok()
            .and_then(|r| doc.dereference(r).ok())
            .and_then(|(_, r)| r.as_dict().ok().cloned());
        let Some(procs) = font
            .get(b"CharProcs")
            .ok()
            .and_then(|p| doc.dereference(p).ok())
            .and_then(|(_, p)| p.as_dict().ok())
        else {
            continue;
        };
        for (_, glyph) in procs.iter() {
            if let Some(id) = stream_ref(doc, glyph) {
                out.push(Child {
                    stream: id,
                    resources: own.clone(),
                });
            }
        }
    }
    out
}

/// Relève les glyphes des chaînes de texte (`Tj`, `TJ`, `'`, `"`) dessinées par une police
/// candidate (`Tf`). `None` au-delà de [`MAX_GLYPH_REFS`].
fn read_strings(
    c: &Candidates,
    fonts: &Dictionary,
    content: &Content,
    usage: &mut Usage,
) -> Option<()> {
    // Police descendante courante, si la police choisie par `Tf` est candidate.
    let mut current: Option<ObjectId> = None;
    for op in &content.operations {
        let strings: Vec<&[u8]> = match op.operator.as_str() {
            "Tf" => {
                current = op
                    .operands
                    .first()
                    .and_then(|n| n.as_name().ok())
                    .and_then(|n| fonts.get(n).ok())
                    .and_then(|f| f.as_reference().ok())
                    .and_then(|f| c.type0.get(&f).copied());
                continue;
            }
            "Tj" | "'" => op
                .operands
                .first()
                .into_iter()
                .filter_map(as_bytes)
                .collect(),
            "\"" => op
                .operands
                .get(2)
                .into_iter()
                .filter_map(as_bytes)
                .collect(),
            "TJ" => op
                .operands
                .first()
                .and_then(|a| a.as_array().ok())
                .map(|items| items.iter().filter_map(as_bytes).collect())
                .unwrap_or_default(),
            _ => continue,
        };
        let Some(cid_font) = current else {
            continue;
        };
        let (Some(font), Some(file)) = (c.cid_fonts.get(&cid_font), c.file_of(cid_font)) else {
            continue;
        };
        let glyphs = usage.glyphs.entry(file).or_default();
        for string in strings {
            // Identity-H : deux octets par CID, gros-boutiste.
            // `as_chunks::<2>()` : des tableaux `[u8; 2]` (un octet impair final est ignoré).
            for pair in string.as_chunks::<2>().0 {
                usage.glyph_refs += 1;
                if usage.glyph_refs > MAX_GLYPH_REFS {
                    return None;
                }
                let cid = u16::from_be_bytes(*pair);
                let gid = match &font.map {
                    None => cid,
                    Some(map) => map.get(usize::from(cid)).copied().unwrap_or(0),
                };
                glyphs.insert(gid);
            }
        }
    }
    Some(())
}

/// Les octets d'une chaîne PDF (littérale ou hexadécimale).
fn as_bytes(object: &Object) -> Option<&[u8]> {
    match object {
        Object::String(bytes, _) => Some(bytes),
        _ => None,
    }
}

/// Rend intouchables les programmes dont une police est citée par un objet que le parcours
/// n'a pas vu : un formulaire `/DR` (un autre lecteur y dessinerait n'importe quel
/// caractère), une police simple bâtie sur le même descripteur, etc.
///
/// Seuls comptent les objets atteignables : un orphelin (laissé par « annuler ») part à
/// l'élagage, il ne dessinera plus rien.
fn mark_foreign_references(doc: &Document, c: &mut Candidates, usage: &Usage) {
    let live = super::reachable(doc);
    let mut foreign: BTreeSet<ObjectId> = BTreeSet::new();
    for (holder, object) in doc.objects.iter().filter(|(id, _)| live.contains(id)) {
        for target in references_in(object) {
            // Programme concerné par cette référence, et le citer est-il permis à `holder` ?
            let (file, allowed) = if c.files.contains_key(&target) {
                (Some(target), c.descriptors.get(holder) == Some(&target))
            } else if let Some(file) = c.descriptors.get(&target) {
                let allowed = c
                    .cid_fonts
                    .get(holder)
                    .is_some_and(|f| f.descriptor == target);
                (Some(*file), allowed)
            } else if c.cid_fonts.contains_key(&target) {
                (c.file_of(target), c.type0.get(holder) == Some(&target))
            } else if let Some(cid_font) = c.type0.get(&target) {
                (c.file_of(*cid_font), usage.holders.contains(holder))
            } else {
                (None, true)
            };
            if let (Some(file), false) = (file, allowed) {
                foreign.insert(file);
            }
        }
    }
    c.untouchable.extend(foreign);
}

/// Les références contenues dans un objet (dictionnaire de flux compris), à toute
/// profondeur, par une pile explicite.
fn references_in(object: &Object) -> Vec<ObjectId> {
    let mut out = Vec::new();
    let mut stack = vec![object];
    while let Some(object) = stack.pop() {
        match object {
            Object::Reference(id) => out.push(*id),
            Object::Array(items) => stack.extend(items.iter()),
            Object::Dictionary(dict) => stack.extend(dict.iter().map(|(_, o)| o)),
            Object::Stream(stream) => stack.extend(stream.dict.iter().map(|(_, o)| o)),
            _ => {}
        }
    }
    out
}

/// Ce qu'il faut écrire pour un programme : tout est calculé **avant** de toucher au
/// document, pour qu'un échec laisse la police intacte.
struct Plan {
    /// Le programme réduit (TrueType, non comprimé).
    program: Vec<u8>,
    /// L'étiquette `XXXXXX` du sous-ensemble.
    tag: String,
    /// Police descendante → nouvelle `/CIDToGIDMap` (deux octets par CID).
    maps: BTreeMap<ObjectId, Vec<u8>>,
    /// Descripteur → CID présents (pour `/CIDSet`).
    cid_sets: BTreeMap<ObjectId, BTreeSet<u16>>,
}

/// Le sous-ensemble du programme `file` réduit à `glyphs` (glyphe 0 toujours gardé), et les
/// tables qui vont avec ; `None` si `subsetter` échoue.
fn plan_subset(c: &Candidates, file: ObjectId, glyphs: &BTreeSet<u16>) -> Option<Plan> {
    let sorted: Vec<u16> = glyphs.iter().copied().collect();
    // [Apprentissage] Les glyphes gardés sont renumérotés 0 (.notdef), 1, 2… dans l'ordre
    // croissant ; `remapper.get(ancien)` donne le nouveau numéro.
    let remapper = subsetter::GlyphRemapper::new_from_glyphs_sorted(&sorted);
    let program = subsetter::subset(DEJAVU_SANS, 0, &remapper).ok()?;
    let mut maps = BTreeMap::new();
    let mut cid_sets: BTreeMap<ObjectId, BTreeSet<u16>> = BTreeMap::new();
    for (id, font) in &c.cid_fonts {
        if c.descriptors.get(&font.descriptor) != Some(&file) {
            continue;
        }
        // Paires (CID, nouveau glyphe) des CID dont le glyphe est gardé.
        let pairs: Vec<(u16, u16)> = match &font.map {
            None => sorted
                .iter()
                .filter_map(|gid| Some((*gid, remapper.get(*gid)?)))
                .collect(),
            Some(map) => map
                .iter()
                .enumerate()
                .filter(|(_, gid)| glyphs.contains(gid))
                .filter_map(|(cid, gid)| Some((u16::try_from(cid).ok()?, remapper.get(*gid)?)))
                .collect(),
        };
        let last = pairs
            .iter()
            .map(|(cid, _)| usize::from(*cid))
            .max()
            .unwrap_or(0);
        let mut bytes = vec![0u8; 2 * (last + 1)];
        for (cid, gid) in &pairs {
            let at = 2 * usize::from(*cid);
            bytes[at..at + 2].copy_from_slice(&gid.to_be_bytes());
        }
        maps.insert(*id, bytes);
        let set = cid_sets.entry(font.descriptor).or_default();
        set.insert(0);
        set.extend(pairs.iter().map(|(cid, _)| *cid));
    }
    Some(Plan {
        program,
        tag: subset_tag(sorted.iter().map(|g| u32::from(*g))),
        maps,
        cid_sets,
    })
}

/// Écrit le sous-ensemble : programme (Flate, `/Length1`), tables `/CIDToGIDMap` et
/// `/CIDSet`, noms étiquetés (`/BaseFont`, `/FontName`). `/W`, `/DW` et `/ToUnicode`
/// restent tels quels : ils sont indexés par CID, et les CID ne changent pas.
fn apply_plan(doc: &mut Document, c: &Candidates, file: ObjectId, plan: Plan) {
    let length1 = i64::try_from(plan.program.len()).unwrap_or(i64::MAX);
    doc.objects.insert(
        file,
        Object::Stream(compressed(
            Dictionary::from_iter(vec![("Length1", Object::Integer(length1))]),
            plan.program,
        )),
    );
    for (cid_font, map) in plan.maps {
        let map_id = doc.add_object(compressed(Dictionary::new(), map));
        if let Ok(dict) = doc.get_dictionary_mut(cid_font) {
            dict.set("CIDToGIDMap", Object::Reference(map_id));
            retag(dict, b"BaseFont", &plan.tag);
        }
    }
    for (descriptor, cids) in plan.cid_sets {
        // Un bit par CID, poids fort d'abord (voir `Type0Writer::write`).
        let last = cids.iter().next_back().map_or(0, |cid| usize::from(*cid));
        let mut bits = vec![0u8; last / 8 + 1];
        for cid in cids.iter().map(|cid| usize::from(*cid)) {
            bits[cid / 8] |= 0x80 >> (cid % 8);
        }
        let set_id = doc.add_object(compressed(Dictionary::new(), bits));
        if let Ok(dict) = doc.get_dictionary_mut(descriptor) {
            dict.set("CIDSet", Object::Reference(set_id));
            retag(dict, b"FontName", &plan.tag);
        }
    }
    let type0: Vec<ObjectId> = c
        .type0
        .iter()
        .filter(|(_, cid_font)| c.file_of(**cid_font) == Some(file))
        .map(|(id, _)| *id)
        .collect();
    for id in type0 {
        if let Ok(dict) = doc.get_dictionary_mut(id) {
            retag(dict, b"BaseFont", &plan.tag);
        }
    }
}

/// Pose l'étiquette `tag` sur le nom `key` (`DejaVuSans` → `ABCDEF+DejaVuSans`), en
/// remplaçant une étiquette déjà présente.
fn retag(dict: &mut Dictionary, key: &[u8], tag: &str) {
    let name = name_of(dict, key).unwrap_or(b"DejaVuSans");
    let already_tagged =
        name.len() > 7 && name[..6].iter().all(u8::is_ascii_uppercase) && name[6] == b'+';
    let bare = if already_tagged { &name[7..] } else { name };
    // [Apprentissage] `bare` emprunte `dict` : on construit le nouveau nom (une copie)
    // avant d'appeler `set`, qui a besoin de `dict` en écriture.
    let mut tagged = format!("{tag}+").into_bytes();
    tagged.extend_from_slice(bare);
    dict.set(key, Object::Name(tagged));
}

/// Le nom rangé sous `key`, s'il y en a un.
fn name_of<'a>(dict: &'a Dictionary, key: &[u8]) -> Option<&'a [u8]> {
    dict.get(key).ok().and_then(|n| n.as_name().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::unicode::UnicodeFont;
    use lopdf::Stream;

    /// Une page dont le contenu dessine `text` (Identity-H) avec une police `Type0` bâtie,
    /// comme le fait PDFium, sur la DejaVu **entière** ; rend aussi le numéro du programme.
    fn doc_with_full_font(content: impl FnOnce(&str) -> String) -> (Document, ObjectId) {
        let base = crate::testing::blank_pdf(595.0, 842.0);
        let mut doc = crate::crypt::load_limited(&base, None).unwrap();
        let file = doc.add_object(compressed(
            Dictionary::from_iter(vec![("Length1", Object::Integer(DEJAVU_SANS.len() as i64))]),
            DEJAVU_SANS.to_vec(),
        ));
        let descriptor = doc.add_object(Dictionary::from_iter(vec![
            ("Type", Object::Name(b"FontDescriptor".to_vec())),
            ("FontName", Object::Name(b"DejaVuSans".to_vec())),
            ("FontFile2", Object::Reference(file)),
        ]));
        let cid_font = doc.add_object(Dictionary::from_iter(vec![
            ("Type", Object::Name(b"Font".to_vec())),
            ("Subtype", Object::Name(b"CIDFontType2".to_vec())),
            ("BaseFont", Object::Name(b"DejaVuSans".to_vec())),
            ("FontDescriptor", Object::Reference(descriptor)),
        ]));
        let type0 = doc.add_object(Dictionary::from_iter(vec![
            ("Type", Object::Name(b"Font".to_vec())),
            ("Subtype", Object::Name(b"Type0".to_vec())),
            ("BaseFont", Object::Name(b"DejaVuSans".to_vec())),
            ("Encoding", Object::Name(b"Identity-H".to_vec())),
            (
                "DescendantFonts",
                Object::Array(vec![Object::Reference(cid_font)]),
            ),
        ]));
        let page_id = *doc.get_pages().get(&1).unwrap();
        let stream = doc.add_object(Stream::new(Dictionary::new(), content("U1").into_bytes()));
        let page = doc.get_dictionary_mut(page_id).unwrap();
        page.set("Contents", Object::Reference(stream));
        page.set(
            "Resources",
            Object::Dictionary(Dictionary::from_iter(vec![(
                "Font",
                Object::Dictionary(Dictionary::from_iter(vec![(
                    "U1",
                    Object::Reference(type0),
                )])),
            )])),
        );
        (doc, file)
    }

    /// `<…>` : les glyphes d'origine de `text` en CID Identity.
    fn hex(text: &str) -> String {
        let font = UnicodeFont::embedded();
        let cids: String = text
            .chars()
            .map(|ch| format!("{:04X}", font.glyph(ch).unwrap()))
            .collect();
        format!("<{cids}>")
    }

    fn glyphs_of(text: &str) -> BTreeSet<u16> {
        let font = UnicodeFont::embedded();
        text.chars().map(|ch| font.glyph(ch).unwrap()).collect()
    }

    #[test]
    fn glyphs_of_tj_and_tj_arrays_are_collected() {
        let (doc, file) = doc_with_full_font(|f| {
            format!(
                "BT /{f} 12 Tf 50 700 Td {} Tj [{} -120 {}] TJ ET",
                hex("Ω"),
                hex("Ж"),
                hex("к")
            )
        });
        let candidates = find_candidates(&doc).unwrap();
        let usage = collect_usage(&doc, &candidates).unwrap();
        assert_eq!(usage.glyphs.get(&file), Some(&glyphs_of("ΩЖк")));
    }

    /// Un formulaire sans ressources propres dessine avec celles de la page qui l'appelle :
    /// dessiné depuis deux pages où `/U1` désigne deux polices, il est lu deux fois. La page 2
    /// (Helvetica) est lue la première (pile) : un formulaire marqué « vu » à ce moment-là
    /// ferait manquer le « Ж » qu'il dessine en DejaVu sur la page 1.
    #[test]
    fn glyphs_of_a_form_using_the_page_resources_are_collected() {
        let (mut doc, file) =
            doc_with_full_font(|f| format!("BT /{f} 12 Tf {} Tj ET /X1 Do", hex("Ω")));
        let form_content = format!("BT /U1 12 Tf {} Tj ET", hex("Ж"));
        let form = doc.add_object(Stream::new(
            Dictionary::from_iter(vec![
                ("Type", Object::Name(b"XObject".to_vec())),
                ("Subtype", Object::Name(b"Form".to_vec())),
                (
                    "BBox",
                    Object::Array(vec![0.into(), 0.into(), 100.into(), 100.into()]),
                ),
            ]),
            form_content.into_bytes(),
        ));
        let xobjects =
            || Object::Dictionary(Dictionary::from_iter(vec![("X1", Object::Reference(form))]));
        let page_id = *doc.get_pages().get(&1).unwrap();
        let page = doc.get_dictionary_mut(page_id).unwrap();
        let Ok(Object::Dictionary(resources)) = page.get_mut(b"Resources") else {
            panic!("ressources en ligne attendues");
        };
        resources.set("XObject", xobjects());
        // Page 2 : même formulaire, mais `/U1` y est Helvetica.
        let helvetica = doc.add_object(Dictionary::from_iter(vec![
            ("Type", Object::Name(b"Font".to_vec())),
            ("Subtype", Object::Name(b"Type1".to_vec())),
            ("BaseFont", Object::Name(b"Helvetica".to_vec())),
        ]));
        let mut second = doc.get_dictionary(page_id).unwrap().clone();
        let contents = doc.add_object(Stream::new(Dictionary::new(), b"/X1 Do".to_vec()));
        second.set("Contents", Object::Reference(contents));
        second.set(
            "Resources",
            Object::Dictionary(Dictionary::from_iter(vec![
                (
                    "Font",
                    Object::Dictionary(Dictionary::from_iter(vec![(
                        "U1",
                        Object::Reference(helvetica),
                    )])),
                ),
                ("XObject", xobjects()),
            ])),
        );
        let second = doc.add_object(second);
        let root = doc
            .catalog()
            .unwrap()
            .get(b"Pages")
            .unwrap()
            .as_reference()
            .unwrap();
        let pages = doc.get_dictionary_mut(root).unwrap();
        let Ok(Object::Array(kids)) = pages.get_mut(b"Kids") else {
            panic!("/Kids attendu");
        };
        kids.push(Object::Reference(second));
        pages.set("Count", Object::Integer(2));
        assert_eq!(doc.get_pages().len(), 2);
        let candidates = find_candidates(&doc).unwrap();
        let usage = collect_usage(&doc, &candidates).unwrap();
        assert_eq!(usage.glyphs.get(&file), Some(&glyphs_of("ΩЖ")));
    }

    #[test]
    fn full_program_is_replaced_by_a_small_tagged_subset() {
        let (mut doc, file) =
            doc_with_full_font(|f| format!("BT /{f} 12 Tf 50 700 Td {} Tj ET", hex("Ωμ")));
        let before = doc
            .get_object(file)
            .unwrap()
            .as_stream()
            .unwrap()
            .content
            .len();
        subset_embedded_fonts(&mut doc).unwrap();
        let stream = doc.get_object(file).unwrap().as_stream().unwrap();
        let after = stream.content.len();
        assert!(
            after < 40 * 1024 && after * 10 < before,
            "{before} → {after}"
        );
        let length1 = stream.dict.get(b"Length1").unwrap().as_i64().unwrap();
        assert!(length1 < DEJAVU_SANS.len() as i64 / 10, "{length1}");
        // Les noms portent l'étiquette, et la table CID → glyphe est écrite.
        let names: Vec<Vec<u8>> = doc
            .objects
            .values()
            .filter_map(|o| o.as_dict().ok())
            .filter_map(|d| d.get(b"BaseFont").or(d.get(b"FontName")).ok())
            .filter_map(|n| n.as_name().ok().map(<[u8]>::to_vec))
            .filter(|n| n.ends_with(b"DejaVuSans"))
            .collect();
        assert_eq!(names.len(), 3, "Type0, descendante, descripteur");
        assert!(
            names.iter().all(|n| n.len() == 17 && n[6] == b'+'),
            "{names:?}"
        );
        assert!(names.windows(2).all(|w| w[0] == w[1]), "même étiquette");
        let cid_font = doc
            .objects
            .values()
            .filter_map(|o| o.as_dict().ok())
            .find(|d| name_of(d, b"Subtype") == Some(b"CIDFontType2"))
            .unwrap();
        let map_id = cid_font
            .get(b"CIDToGIDMap")
            .unwrap()
            .as_reference()
            .unwrap();
        let map = crate::crypt::stream_bytes(doc.get_object(map_id).unwrap().as_stream().unwrap())
            .unwrap();
        // Ω et μ, glyphes d'origine, deviennent 1 et 2 (ordre croissant des anciens numéros).
        let mut gids: Vec<u16> = glyphs_of("Ωμ").into_iter().collect();
        gids.sort_unstable();
        for (new, old) in gids.iter().enumerate() {
            let at = 2 * usize::from(*old);
            assert_eq!(u16::from_be_bytes([map[at], map[at + 1]]), new as u16 + 1);
        }
    }

    #[test]
    fn a_font_also_listed_in_the_form_resources_is_left_whole() {
        let (mut doc, file) =
            doc_with_full_font(|f| format!("BT /{f} 12 Tf 50 700 Td {} Tj ET", hex("Ω")));
        let page_id = *doc.get_pages().get(&1).unwrap();
        let fonts = doc
            .get_dictionary(page_id)
            .unwrap()
            .get(b"Resources")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Font")
            .unwrap()
            .clone();
        let acroform = doc.add_object(Dictionary::from_iter(vec![
            ("Fields", Object::Array(vec![])),
            (
                "DR",
                Object::Dictionary(Dictionary::from_iter(vec![("Font", fonts)])),
            ),
        ]));
        doc.catalog_mut()
            .unwrap()
            .set("AcroForm", Object::Reference(acroform));
        let before = doc
            .get_object(file)
            .unwrap()
            .as_stream()
            .unwrap()
            .content
            .clone();
        subset_embedded_fonts(&mut doc).unwrap();
        let after = &doc.get_object(file).unwrap().as_stream().unwrap().content;
        assert_eq!(after, &before, "police citée par /DR : intacte");
    }

    #[test]
    fn unreadable_content_leaves_the_font_whole() {
        let (mut doc, file) =
            doc_with_full_font(|f| format!("BT /{f} 12 Tf {} Tj ET ( non fermée", hex("Ω")));
        let before = doc
            .get_object(file)
            .unwrap()
            .as_stream()
            .unwrap()
            .content
            .len();
        subset_embedded_fonts(&mut doc).unwrap();
        let after = doc
            .get_object(file)
            .unwrap()
            .as_stream()
            .unwrap()
            .content
            .len();
        assert_eq!(after, before);
    }
}
