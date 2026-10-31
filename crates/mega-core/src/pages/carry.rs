// SPDX-License-Identifier: GPL-3.0-or-later
//! Report du catalogue après une copie de pages par PDFium (audit E3).
//!
//! `merge`, `extract_pages` et `split` assemblent un document **neuf** avec PDFium : seules les
//! pages et ce qu'elles citent sont copiées. Tout ce qui vit au niveau du catalogue est perdu
//! (formulaire, signets, pièces jointes, destinations nommées, étiquettes de pages, calques,
//! langue, XMP…), et PDFium recopie les clés `/Parent`, `/Prev` et `/First` **sans les
//! renuméroter** : le `/Parent` d'un widget copié désigne alors un objet quelconque du nouveau
//! document, et le formulaire est corrompu. Ce module relit la source et la copie avec `lopdf`
//! et reporte ce qui concerne les pages copiées, structure par structure. Chaque étape est une
//! fonction pure sur deux `lopdf::Document`, testée sans PDFium.
//!
//! Jamais reporté : les scripts du catalogue (`/Names /JavaScript`, `/OpenAction`, `/AA` : on
//! n'importe pas de code) et l'arbre de structure (`/StructTreeRoot`, qui ne se découpe pas
//! page par page) ; les `/StructParents` qui le citaient sont retirés.
//!
//! \[Apprentissage\] **Clone avec table de correspondance et ensemble des visités.** Copier un
//! objet d'un document à l'autre, c'est copier aussi tout ce qu'il cite, et ce graphe peut
//! boucler (un champ cite ses enfants, qui citent leur parent). La table `cache`
//! (`HashMap<ObjectId, ObjectId>`, numéro source → numéro cible) joue deux rôles : elle dit où
//! est déjà la copie d'un objet (la référence est réécrite vers elle) et, du même coup, quels
//! objets ont déjà été visités (aucun n'est copié deux fois : la boucle s'arrête). Le travail
//! restant est tenu dans un `Vec` (**pile explicite**) au lieu d'un appel récursif : la
//! profondeur d'un graphe venu du fichier n'est pas bornée, une récursion pourrait épuiser la
//! pile du programme. Voir `docs/apprentissage/concepts/complexite-et-collections.md`.

use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Document as LoDocument, Object, ObjectId};

mod acroform;
mod dests;
mod labels;
mod layers;

pub(crate) use acroform::carry_acroform;
pub(crate) use dests::{carry_named_dests, carry_outlines};
pub(crate) use labels::carry_page_labels;
pub(crate) use layers::carry_oc_properties;

/// Profondeur maximale suivie dans un arbre venu du fichier (champs, signets, arbres de noms,
/// `/Order` des calques) : au-delà, la branche est ignorée. Même borne que la lecture des
/// signets (`outline::MAX_OUTLINE_DEPTH`).
pub(crate) const MAX_DEPTH: usize = 64;

/// Correspondance entre les pages de la source et leurs copies dans le document cible.
///
/// Pourquoi : PDFium copie les pages dans l'ordre demandé, mais sous d'autres numéros d'objet ;
/// toute référence à une page (destination d'un signet, d'un lien) doit être réécrite vers la
/// copie, ou abandonnée si la page n'a pas été copiée.
pub(crate) struct PageMap {
    /// (page source, page cible), dans l'ordre des pages copiées.
    pairs: Vec<(ObjectId, ObjectId)>,
    /// Index de `pairs` par page source (la première copie) : [`PageMap::target`] est appelé
    /// pour chaque référence clonée, une recherche linéaire serait quadratique sur une grosse
    /// fusion.
    index: HashMap<ObjectId, ObjectId>,
}

impl PageMap {
    /// La `i`-ème page copiée de `dst` ↔ la page `src_pages[i]` de `src`. Les pages copiées
    /// commencent à l'index `dst_first` de `dst` : 0 pour une extraction, le nombre de pages
    /// des sources précédentes pour une fusion. Un index hors bornes est ignoré.
    pub(crate) fn by_index(
        src: &LoDocument,
        dst: &LoDocument,
        src_pages: &[u16],
        dst_first: usize,
    ) -> PageMap {
        let src_ids: Vec<ObjectId> = src.get_pages().into_values().collect();
        let dst_ids: Vec<ObjectId> = dst.get_pages().into_values().collect();
        let pairs: Vec<(ObjectId, ObjectId)> = src_pages
            .iter()
            .enumerate()
            .filter_map(|(i, &page)| {
                let from = *src_ids.get(usize::from(page))?;
                let to = *dst_ids.get(dst_first + i)?;
                Some((from, to))
            })
            .collect();
        PageMap::new(pairs)
    }

    /// Correspondance à partir de ses paires ; une page copiée deux fois désigne sa première
    /// copie.
    fn new(pairs: Vec<(ObjectId, ObjectId)>) -> PageMap {
        let mut index = HashMap::new();
        for &(from, to) in &pairs {
            index.entry(from).or_insert(to);
        }
        PageMap { pairs, index }
    }

    /// Copie de la page source `src`, si elle a été copiée (la première, si elle l'a été
    /// plusieurs fois).
    pub(crate) fn target(&self, src: ObjectId) -> Option<ObjectId> {
        self.index.get(&src).copied()
    }
}

/// Copie dans `dst` de l'objet `obj` de `src` et de **tout ce qu'il cite**, une seule fois
/// chacun (`cache`, voir l'en-tête du module). Une référence vers une page source devient la
/// page cible (`map`), ou `null` si la page n'a pas été copiée ; une référence vers l'arbre des
/// pages ou le catalogue devient `null` (on ne recopie jamais la structure du document).
pub(crate) fn clone_object(
    src: &LoDocument,
    dst: &mut LoDocument,
    obj: &Object,
    map: &PageMap,
    cache: &mut HashMap<ObjectId, ObjectId>,
) -> Object {
    // Objets à copier : (numéro source, numéro déjà réservé dans `dst`).
    let mut pending: Vec<(ObjectId, ObjectId)> = Vec::new();
    let copy = rewrite_references(src, dst, obj.clone(), map, cache, &mut pending);
    while let Some((from, to)) = pending.pop() {
        let object = match src.get_object(from) {
            Ok(object) => rewrite_references(src, dst, object.clone(), map, cache, &mut pending),
            Err(_) => Object::Null,
        };
        dst.objects.insert(to, object);
    }
    copy
}

/// `object` (copie d'un objet de `src`) dont chaque référence est réécrite vers `dst` ; un
/// objet cité pour la première fois reçoit un numéro dans `dst` et rejoint `pending`.
fn rewrite_references(
    src: &LoDocument,
    dst: &mut LoDocument,
    mut object: Object,
    map: &PageMap,
    cache: &mut HashMap<ObjectId, ObjectId>,
    pending: &mut Vec<(ObjectId, ObjectId)>,
) -> Object {
    visit_references(&mut object, |reference| {
        if let Object::Reference(id) = reference {
            let id = *id;
            *reference = reference_in_target(src, dst, id, map, cache, pending);
        }
    });
    object
}

/// La référence de `dst` qui remplace la référence `id` de `src` (voir [`clone_object`]).
fn reference_in_target(
    src: &LoDocument,
    dst: &mut LoDocument,
    id: ObjectId,
    map: &PageMap,
    cache: &mut HashMap<ObjectId, ObjectId>,
    pending: &mut Vec<(ObjectId, ObjectId)>,
) -> Object {
    if let Some(page) = map.target(id) {
        return Object::Reference(page);
    }
    if let Some(&copy) = cache.get(&id) {
        return Object::Reference(copy);
    }
    match src.get_object(id) {
        Ok(object) if !is_structural(object) => {
            let copy = dst.new_object_id();
            cache.insert(id, copy);
            pending.push((id, copy));
            Object::Reference(copy)
        }
        // Page non copiée, arbre des pages, catalogue, ou référence morte.
        _ => Object::Null,
    }
}

/// Appelle `f` sur chaque référence contenue dans `object`, à toute profondeur (pile
/// explicite, voir l'en-tête du module).
fn visit_references(object: &mut Object, mut f: impl FnMut(&mut Object)) {
    let mut stack: Vec<&mut Object> = vec![object];
    while let Some(current) = stack.pop() {
        match current {
            Object::Reference(_) => f(current),
            Object::Array(items) => stack.extend(items.iter_mut()),
            Object::Dictionary(dict) => stack.extend(dict.iter_mut().map(|(_, value)| value)),
            Object::Stream(stream) => {
                stack.extend(stream.dict.iter_mut().map(|(_, value)| value));
            }
            _ => {}
        }
    }
}

/// `/Type` d'un dictionnaire (ou d'un dictionnaire de flux).
fn type_of(object: &Object) -> Option<&[u8]> {
    let dict = match object {
        Object::Dictionary(dict) => dict,
        Object::Stream(stream) => &stream.dict,
        _ => return None,
    };
    dict.get(b"Type").and_then(Object::as_name).ok()
}

/// Page, nœud de l'arbre des pages ou catalogue : la structure du document, jamais copiée.
fn is_structural(object: &Object) -> bool {
    matches!(type_of(object), Some(b"Page" | b"Pages" | b"Catalog"))
}

/// Clés que PDFium recopie **sans les renuméroter** (`CPDF_PageOrganizer::UpdateReference`) :
/// dans la copie, leur valeur désigne un objet quelconque ; on ne les apparie jamais.
const NOT_RENUMBERED: [&[u8]; 3] = [b"Parent", b"Prev", b"First"];

/// Remplit `cache` avec ce que PDFium a **déjà** copié, en parcourant en parallèle chaque page
/// source et sa copie (ressources, annotations, contenus, calques cités…) : le report réutilise
/// ces copies au lieu d'en créer de nouvelles (la police d'un `/DR` est celle des pages, le
/// calque de `/OCProperties` est celui que citent les pages).
///
/// Rend les doublons (copie en trop → copie retenue) : l'extraction copie les pages une à une,
/// et PDFium recopie alors un objet partagé (police, calque) une fois par page. Une annotation
/// n'est jamais fusionnée avec une autre copie : une page extraite deux fois (`[0, 0]`) garde
/// ses propres annotations, rendues à part (`repeated`) pour que leur `/Parent` soit réparé.
fn pair_copies(
    src: &LoDocument,
    dst: &LoDocument,
    map: &PageMap,
    cache: &mut HashMap<ObjectId, ObjectId>,
) -> Copies {
    let mut duplicates = HashMap::new();
    let mut repeated = Vec::new();
    let mut stack: Vec<(&Object, &Object)> = Vec::new();
    for &(from, to) in &map.pairs {
        let (Ok(page), Ok(copy)) = (src.get_dictionary(from), dst.get_dictionary(to)) else {
            continue;
        };
        push_pairs(page, copy, &mut stack);
        // Ressources héritées d'un nœud `/Pages` : PDFium les recopie sur la page.
        if !page.has(b"Resources")
            && let (Some(inherited), Ok(copied)) =
                (inherited_resources(src, from), copy.get(b"Resources"))
        {
            stack.push((inherited, copied));
        }
    }
    while let Some(pair) = stack.pop() {
        match pair {
            (Object::Reference(from), Object::Reference(to)) => {
                if map.target(*from).is_some() {
                    continue;
                }
                if let Some(&known) = cache.get(from) {
                    if known == *to {
                        continue;
                    }
                    if src.get_dictionary(*from).is_ok_and(is_annotation) {
                        repeated.push((*from, *to));
                    } else {
                        duplicates.entry(*to).or_insert(known);
                    }
                    continue;
                }
                let (Ok(object), Ok(copy)) = (src.get_object(*from), dst.get_object(*to)) else {
                    continue;
                };
                if is_structural(object) {
                    continue;
                }
                cache.insert(*from, *to);
                stack.push((object, copy));
            }
            (Object::Dictionary(a), Object::Dictionary(b)) => push_pairs(a, b, &mut stack),
            (Object::Stream(a), Object::Stream(b)) => push_pairs(&a.dict, &b.dict, &mut stack),
            (Object::Array(a), Object::Array(b)) if a.len() == b.len() => {
                stack.extend(a.iter().zip(b));
            }
            _ => {}
        }
    }
    Copies {
        duplicates,
        repeated,
    }
}

/// Ce que [`pair_copies`] a trouvé en plus de `cache`.
struct Copies {
    /// Copie en trop d'un objet partagé → copie retenue.
    duplicates: HashMap<ObjectId, ObjectId>,
    /// (annotation source, copie supplémentaire) d'une page copiée plusieurs fois.
    repeated: Vec<(ObjectId, ObjectId)>,
}

/// Une annotation : un dictionnaire qui porte `/Subtype` et `/Rect` (§12.5.2).
fn is_annotation(dict: &Dictionary) -> bool {
    dict.has(b"Subtype") && dict.has(b"Rect")
}

/// Empile les valeurs de même clé de `a` (source) et `b` (copie), sauf les clés que PDFium ne
/// renumérote pas.
fn push_pairs<'a>(a: &'a Dictionary, b: &'a Dictionary, stack: &mut Vec<(&'a Object, &'a Object)>) {
    for (key, value) in a.iter() {
        if NOT_RENUMBERED.contains(&key.as_slice()) {
            continue;
        }
        if let Ok(copied) = b.get(key) {
            stack.push((value, copied));
        }
    }
}

/// `/Resources` hérité par la page `page` d'un de ses ancêtres (boucle bornée, §7.7.3.4).
fn inherited_resources(src: &LoDocument, page: ObjectId) -> Option<&Object> {
    let mut seen = HashSet::new();
    let mut current = src.get_dictionary(page).ok()?;
    for _ in 0..MAX_DEPTH {
        if let Ok(resources) = current.get(b"Resources") {
            return Some(resources);
        }
        let parent = current.get(b"Parent").and_then(Object::as_reference).ok()?;
        if !seen.insert(parent) {
            return None;
        }
        current = src.get_dictionary(parent).ok()?;
    }
    None
}

/// Fait pointer toute référence à une copie en double vers la copie retenue ; les doublons,
/// devenus orphelins, sont élagués à l'enregistrement (`cleanup::before_save`).
fn unify_duplicates(dst: &mut LoDocument, duplicates: &HashMap<ObjectId, ObjectId>) {
    if duplicates.is_empty() {
        return;
    }
    for object in dst.objects.values_mut() {
        visit_references(object, |reference| {
            if let Object::Reference(id) = reference
                && let Some(&kept) = duplicates.get(id)
            {
                *id = kept;
            }
        });
    }
}

/// Répare les annotations copiées par PDFium : un `/Parent` (popup → note, widget → champ) non
/// renuméroté désigne un objet quelconque ; il est réécrit vers la copie s'il y en a une, sinon
/// retiré (le widget le retrouve dans [`carry_acroform`]). Les destinations des liens sont
/// reposées par `dests::carry_link_target`.
fn fix_annotations(
    src: &LoDocument,
    dst: &mut LoDocument,
    map: &PageMap,
    cache: &mut HashMap<ObjectId, ObjectId>,
    named: &dests::Named,
    repeated: &[(ObjectId, ObjectId)],
) {
    let mut copies: Vec<(ObjectId, ObjectId)> = cache.iter().map(|(&a, &b)| (a, b)).collect();
    copies.extend_from_slice(repeated);
    // Ordre fixe (celui d'une `HashMap` varie d'une exécution à l'autre) : à entrée égale,
    // les objets créés reçoivent les mêmes numéros, et le fichier produit est le même.
    copies.sort_unstable();
    for (from, to) in copies {
        let Ok(annotation) = src.get_dictionary(from) else {
            continue;
        };
        if !is_annotation(annotation) {
            continue;
        }
        if annotation.has(b"Parent") {
            let parent = annotation
                .get(b"Parent")
                .and_then(Object::as_reference)
                .ok()
                .and_then(|p| cache.get(&p).copied());
            if let Ok(copy) = dst.get_dictionary_mut(to) {
                match parent {
                    Some(parent) => copy.set("Parent", Object::Reference(parent)),
                    None => {
                        copy.remove(b"Parent");
                    }
                }
            }
        }
        if annotation.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Link") {
            dests::carry_link_target(src, dst, map, cache, named, from, to);
        }
    }
}

/// Reporte les entrées simples du catalogue (`/Lang`, `/ViewerPreferences`, `/PageMode`,
/// `/PageLayout`, `/Metadata`, `/OutputIntents`) et les pièces jointes
/// (`/Names /EmbeddedFiles`). Lors d'une fusion, la première source qui porte une entrée
/// l'emporte ; une pièce jointe dont le nom est déjà pris est renommée (`notes (2).txt`).
///
/// Jamais `/Names /JavaScript`, `/OpenAction` ni `/AA` : on n'importe pas de script.
pub(crate) fn carry_simple(
    src: &LoDocument,
    dst: &mut LoDocument,
    map: &PageMap,
    cache: &mut HashMap<ObjectId, ObjectId>,
) {
    // Pas `/MarkInfo` : `/Marked true` sans `/StructTreeRoot` (jamais reporté) déclarerait
    // balisé un document qui ne l'est plus (PDF/UA, PDF/A-1a).
    const KEYS: [&str; 6] = [
        "Lang",
        "ViewerPreferences",
        "PageMode",
        "PageLayout",
        "Metadata",
        "OutputIntents",
    ];
    let Ok(catalog) = src.catalog() else {
        return;
    };
    for key in KEYS {
        let Ok(value) = catalog.get(key.as_bytes()) else {
            continue;
        };
        if dst.catalog().is_ok_and(|c| c.has(key.as_bytes())) {
            continue;
        }
        let copy = clone_object(src, dst, value, map, cache);
        if let Ok(target) = dst.catalog_mut() {
            target.set(key, copy);
        }
    }
    let files = name_tree(src, b"EmbeddedFiles");
    if files.is_empty() {
        return;
    }
    let mut entries = name_tree(dst, b"EmbeddedFiles");
    let mut taken: Vec<String> = entries.iter().map(|(key, _)| key_text(key)).collect();
    for (key, value) in files {
        let wanted = key_text(&key);
        let name = crate::attachments::unique_name(&wanted, &taken);
        let mut value = clone_object(src, dst, &value, map, cache);
        let key = if name == wanted {
            key
        } else {
            rename_file_spec(dst, &mut value, &name);
            crate::forms::write::pdf_text(&name)
        };
        taken.push(name);
        entries.push((key, value));
    }
    set_name_tree(dst, b"EmbeddedFiles", entries);
}

/// Renomme le descripteur de fichier `spec` (direct ou référencé dans `dst`) : les lecteurs
/// affichent son nom (`/UF`, `/F`), pas la clé de l'arbre.
fn rename_file_spec(dst: &mut LoDocument, spec: &mut Object, name: &str) {
    let dict = match spec {
        Object::Dictionary(dict) => Some(dict),
        Object::Reference(id) => dst.get_dictionary_mut(*id).ok(),
        _ => None,
    };
    if let Some(dict) = dict {
        dict.set("UF", crate::forms::write::pdf_text(name));
        dict.set("F", crate::forms::write::pdf_text(name));
    }
}

/// Texte d'une clé d'arbre de noms (chaîne PDF).
fn key_text(key: &Object) -> String {
    key.as_str()
        .map(crate::properties::decode_text_string)
        .unwrap_or_default()
}

/// Retire `/StructParents` des pages et des XObjects, `/StructParent` des annotations, quand
/// `dst` n'a pas d'arbre de structure : ces numéros citeraient un `/ParentTree` absent.
pub(crate) fn strip_struct_parents(dst: &mut LoDocument) {
    if dst.catalog().is_ok_and(|c| c.has(b"StructTreeRoot")) {
        return;
    }
    for object in dst.objects.values_mut() {
        let dict = match object {
            Object::Dictionary(dict) => dict,
            Object::Stream(stream) => &mut stream.dict,
            _ => continue,
        };
        dict.remove(b"StructParents");
        dict.remove(b"StructParent");
        // Annotations écrites directement dans `/Annots` (forme qu'emploie PDFium).
        if let Ok(annots) = dict.get_mut(b"Annots").and_then(Object::as_array_mut) {
            for annotation in annots.iter_mut() {
                if let Ok(annotation) = annotation.as_dict_mut() {
                    annotation.remove(b"StructParent");
                }
            }
        }
    }
}

/// Reporte sur `dst` (document neuf assemblé par PDFium) tout ce que le catalogue de `src`
/// dit des pages `src_pages`, copiées dans `dst` à partir de l'index `dst_first`.
/// `field_suffix` renomme un champ de formulaire dont le nom est déjà pris dans `dst`
/// (fusion : `" (2)"` pour la deuxième source…).
///
/// Ne renvoie pas d'erreur : une structure illisible de la source n'est simplement pas
/// reportée, la copie de PDFium reste valable.
pub(crate) fn carry_catalog(
    src: &LoDocument,
    dst: &mut LoDocument,
    src_pages: &[u16],
    dst_first: usize,
    field_suffix: Option<&str>,
) {
    let map = PageMap::by_index(src, dst, src_pages, dst_first);
    let mut cache = HashMap::new();
    let copies = pair_copies(src, dst, &map, &mut cache);
    unify_duplicates(dst, &copies.duplicates);
    let named = dests::named_destinations(src);
    fix_annotations(src, dst, &map, &mut cache, &named, &copies.repeated);
    carry_simple(src, dst, &map, &mut cache);
    carry_acroform(src, dst, &map, &mut cache, field_suffix);
    carry_named_dests(src, dst, &map, &mut cache);
    carry_outlines(src, dst, &map, &mut cache);
    carry_page_labels(src, dst, src_pages, dst_first);
    carry_oc_properties(src, dst, &cache);
    strip_struct_parents(dst);
}

// --- Arbres de noms et de nombres ---------------------------------------------------------

/// Entrées (clé, valeur) d'un arbre de noms ou de nombres (§7.9.6, §7.9.7) de racine `root`,
/// dans l'ordre de l'arbre : feuilles `leaf` (`Names` ou `Nums`), nœuds `/Kids` (pile
/// explicite, nœuds visités une fois, profondeur bornée).
pub(crate) fn tree_entries(doc: &LoDocument, root: &Object, leaf: &[u8]) -> Vec<(Object, Object)> {
    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    let mut stack: Vec<(&Object, usize)> = vec![(root, 0)];
    while let Some((node, depth)) = stack.pop() {
        if let Object::Reference(id) = node
            && !seen.insert(*id)
        {
            continue;
        }
        let Ok(Ok(dict)) = doc.dereference(node).map(|(_, n)| n.as_dict()) else {
            continue;
        };
        if let Ok(items) = dict.get(leaf).and_then(Object::as_array) {
            let (pairs, _odd) = items.as_chunks::<2>();
            for [key, value] in pairs {
                entries.push((key.clone(), value.clone()));
            }
        }
        if depth < MAX_DEPTH
            && let Ok(kids) = dict.get(b"Kids").and_then(Object::as_array)
        {
            stack.extend(kids.iter().rev().map(|kid| (kid, depth + 1)));
        }
    }
    entries
}

/// Dictionnaire `/Names` du catalogue (direct ou référencé).
fn names_dict(doc: &LoDocument) -> Option<&Dictionary> {
    catalog_dict(doc, b"Names")
}

/// Entrées de l'arbre `/Names /<key>` de `doc` (`Dests`, `EmbeddedFiles`…).
fn name_tree(doc: &LoDocument, key: &[u8]) -> Vec<(Object, Object)> {
    names_dict(doc)
        .and_then(|names| names.get(key).ok())
        .map(|root| tree_entries(doc, root, b"Names"))
        .unwrap_or_default()
}

/// Écrit `/Names /<key>` comme un arbre à une seule feuille, clés triées (§7.9.6).
fn set_name_tree(dst: &mut LoDocument, key: &[u8], mut entries: Vec<(Object, Object)>) {
    entries.sort_by(|a, b| a.0.as_str().ok().cmp(&b.0.as_str().ok()));
    let names: Vec<Object> = entries.into_iter().flat_map(|(k, v)| [k, v]).collect();
    let mut tree = Dictionary::new();
    tree.set("Names", names);
    if let Some(dict) = names_dict_mut(dst) {
        dict.set(key.to_vec(), tree);
    }
}

/// Dictionnaire `/Names` du catalogue, créé (direct) s'il n'existe pas.
pub(crate) fn names_dict_mut(doc: &mut LoDocument) -> Option<&mut Dictionary> {
    let referenced = doc
        .catalog()
        .ok()?
        .get(b"Names")
        .and_then(Object::as_reference)
        .ok();
    if let Some(id) = referenced {
        return doc.get_dictionary_mut(id).ok();
    }
    let catalog = doc.catalog_mut().ok()?;
    if !catalog.get(b"Names").is_ok_and(|n| n.as_dict().is_ok()) {
        catalog.set("Names", Dictionary::new());
    }
    catalog.get_mut(b"Names").ok()?.as_dict_mut().ok()
}

/// Dictionnaire du catalogue à la clé `key`, déréférencé.
fn catalog_dict<'a>(doc: &'a LoDocument, key: &[u8]) -> Option<&'a Dictionary> {
    let value = doc.catalog().ok()?.get(key).ok()?;
    doc.dereference(value).ok()?.1.as_dict().ok()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use lopdf::{Dictionary, Document as LoDocument, Object, ObjectId, dictionary};

    use super::*;
    use crate::Document;
    use crate::testing;

    fn lo(bytes: &[u8]) -> LoDocument {
        crate::crypt::load_limited(bytes, None).unwrap()
    }

    /// Ce que PDFium seul produit en copiant `pages` dans un document neuf (sans la passe).
    fn raw_copy(bytes: Vec<u8>, pages: &[u16]) -> LoDocument {
        let doc = Document::from_bytes(bytes, None).unwrap();
        lo(&doc.copy_pages_raw(pages).unwrap())
    }

    /// Ce que PDFium seul produit en fusionnant `sources` (sans la passe).
    fn raw_merge(sources: &[Vec<u8>]) -> LoDocument {
        let docs: Vec<Document> = sources
            .iter()
            .map(|b| Document::from_bytes(b.clone(), None).unwrap())
            .collect();
        let refs: Vec<&Document> = docs.iter().collect();
        lo(&crate::pages::merge_raw(&refs).unwrap())
    }

    fn simple() -> Vec<u8> {
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/simple.pdf"
        ))
        .unwrap()
    }

    fn dict<'a>(doc: &'a LoDocument, object: &'a Object) -> &'a Dictionary {
        doc.dereference(object).unwrap().1.as_dict().unwrap()
    }

    /// Vrai si le chemin de clés `path` existe à partir du catalogue.
    fn has_key_path(doc: &LoDocument, path: &[&str]) -> bool {
        let mut current = doc.catalog().unwrap();
        for (i, key) in path.iter().enumerate() {
            let Ok(value) = current.get(key.as_bytes()) else {
                return false;
            };
            if i + 1 == path.len() {
                return true;
            }
            match doc.dereference(value).map(|(_, o)| o.as_dict()) {
                Ok(Ok(next)) => current = next,
                _ => return false,
            }
        }
        true
    }

    fn text(object: &Object) -> String {
        crate::properties::decode_text_string(object.as_str().unwrap())
    }

    /// `/T` des champs de premier niveau de `/AcroForm /Fields`.
    fn top_level_names(doc: &LoDocument) -> Vec<String> {
        let form = dict(doc, doc.catalog().unwrap().get(b"AcroForm").unwrap());
        form.get(b"Fields")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|f| text(dict(doc, f).get(b"T").unwrap()))
            .collect()
    }

    fn pages(doc: &LoDocument) -> Vec<ObjectId> {
        doc.get_pages().into_values().collect()
    }

    #[test]
    fn carry_catalog_keeps_the_seventeen_structures_of_rich_catalog() {
        let bytes = testing::rich_catalog_pdf("1.7", None);
        let src = lo(&bytes);
        let mut dst = raw_copy(bytes, &[0, 1, 2]);
        assert!(
            !has_key_path(&dst, &["AcroForm"]),
            "PDFium seul perd le formulaire"
        );
        carry_catalog(&src, &mut dst, &[0, 1, 2], 0, None);
        for path in [
            &["Names", "EmbeddedFiles"][..],
            &["Outlines"],
            &["AcroForm"],
            &["Dests"],
            &["Names", "Dests"],
            &["PageLabels"],
            &["Lang"],
            &["ViewerPreferences"],
            &["PageMode"],
            &["Metadata"],
            &["OutputIntents"],
            &["OCProperties"],
        ] {
            assert!(has_key_path(&dst, path), "{path:?} manque après le report");
        }
        for path in [
            &["Names", "JavaScript"][..],
            &["OpenAction"],
            &["AA"],
            &["StructTreeRoot"],
            // `/Marked true` sans arbre de structure serait une fausse déclaration (PDF/UA).
            &["MarkInfo"],
        ] {
            assert!(
                !has_key_path(&dst, path),
                "{path:?} ne doit pas être reporté"
            );
        }
        for page in pages(&dst) {
            let page = dst.get_dictionary(page).unwrap();
            assert!(
                !page.has(b"StructParents"),
                "/StructParents sans arbre de structure"
            );
        }
    }

    #[test]
    fn widgets_parent_points_to_a_field_after_merge() {
        let (a, b) = (testing::form_parent_kid_pdf(), simple());
        let (src_a, src_b) = (lo(&a), lo(&b));
        let b_pages: Vec<u16> = (0..src_b.get_pages().len() as u16).collect();
        let mut dst = raw_merge(&[a, b]);
        carry_catalog(&src_a, &mut dst, &[0], 0, None);
        carry_catalog(&src_b, &mut dst, &b_pages, 1, Some(" (2)"));
        let widgets: Vec<&Dictionary> = dst
            .objects
            .values()
            .filter_map(|o| o.as_dict().ok())
            .filter(|d| d.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Widget"))
            .collect();
        assert_eq!(widgets.len(), 1);
        for widget in widgets {
            let parent = dict(&dst, widget.get(b"Parent").unwrap());
            assert!(
                parent.has(b"T") || parent.has(b"FT"),
                "le /Parent du widget doit être un champ : {parent:?}"
            );
        }
        assert_eq!(top_level_names(&dst), ["nom"]);
    }

    #[test]
    fn outlines_to_dropped_pages_are_removed_and_the_rest_renumbered() {
        let bytes = testing::rich_catalog_pdf("1.7", None);
        let src = lo(&bytes);
        let mut dst = raw_copy(bytes, &[1]);
        carry_catalog(&src, &mut dst, &[1], 0, None);
        let root = dict(&dst, dst.catalog().unwrap().get(b"Outlines").unwrap());
        assert_eq!(root.get(b"Count").unwrap().as_i64().unwrap(), 1);
        assert_eq!(root.get(b"First").unwrap(), root.get(b"Last").unwrap());
        let item = dict(&dst, root.get(b"First").unwrap());
        assert_eq!(text(item.get(b"Title").unwrap()), "Vers chap2");
        assert!(!item.has(b"Next") && !item.has(b"Prev"));
        let dest = item.get(b"Dest").unwrap().as_array().unwrap();
        assert_eq!(dest[0], Object::Reference(pages(&dst)[0]));
    }

    #[test]
    fn duplicate_field_names_get_a_suffix_on_merge() {
        let form = testing::form_pdf();
        let src = lo(&form);
        let mut dst = raw_merge(&[form.clone(), form]);
        carry_catalog(&src, &mut dst, &[0], 0, None);
        carry_catalog(&src, &mut dst, &[0], 1, Some(" (2)"));
        let names = top_level_names(&dst);
        assert!(names.iter().any(|n| n == "nom"), "{names:?}");
        assert!(names.iter().any(|n| n == "nom (2)"), "{names:?}");
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "noms en double : {names:?}");
        assert_eq!(names.len(), 14);
    }

    #[test]
    fn clone_object_handles_cycles_without_overflow() {
        // Une chaîne de 50 000 objets refermée en boucle, et un dictionnaire qui se cite.
        let mut src = LoDocument::with_version("1.7");
        let first = src.new_object_id();
        let mut previous = first;
        for _ in 1..50_000 {
            let next = src.new_object_id();
            src.objects
                .insert(previous, Object::Dictionary(dictionary! { "Next" => next }));
            previous = next;
        }
        src.objects.insert(
            previous,
            Object::Dictionary(dictionary! { "Next" => first, "Self" => previous }),
        );
        let mut dst = LoDocument::with_version("1.7");
        let map = PageMap::new(Vec::new());
        let mut cache = std::collections::HashMap::new();
        let cloned = clone_object(&src, &mut dst, &Object::Reference(first), &map, &mut cache);
        assert_eq!(
            dst.objects.len(),
            50_000,
            "chaque objet cloné une seule fois"
        );
        // Suivre la chaîne clonée ramène au point de départ.
        let start = cloned.as_reference().unwrap();
        let mut at = start;
        for _ in 0..50_000 {
            at = dst
                .get_dictionary(at)
                .unwrap()
                .get(b"Next")
                .unwrap()
                .as_reference()
                .unwrap();
        }
        assert_eq!(at, start);
    }

    #[test]
    fn extracted_page_keeps_its_layer_label_and_link_targets() {
        let bytes = testing::rich_catalog_pdf("1.7", None);
        let src = lo(&bytes);
        // Page 2 seule : son calque, l'étiquette « 1 » (décimal, deuxième plage).
        let mut dst = raw_copy(bytes.clone(), &[1]);
        carry_catalog(&src, &mut dst, &[1], 0, None);
        let oc = dict(&dst, dst.catalog().unwrap().get(b"OCProperties").unwrap());
        let ocgs = oc.get(b"OCGs").unwrap().as_array().unwrap();
        assert_eq!(ocgs.len(), 1);
        let page = dst.get_dictionary(pages(&dst)[0]).unwrap();
        let resources = dict(&dst, page.get(b"Resources").unwrap());
        let properties = dict(&dst, resources.get(b"Properties").unwrap());
        assert_eq!(properties.get(b"oc1").unwrap(), &ocgs[0]);
        let labels = dict(&dst, dst.catalog().unwrap().get(b"PageLabels").unwrap());
        let nums = labels.get(b"Nums").unwrap().as_array().unwrap();
        assert_eq!(nums.len(), 2);
        assert_eq!(nums[0], Object::Integer(0));
        let range = dict(&dst, &nums[1]);
        assert_eq!(range.get(b"S").unwrap().as_name().unwrap(), b"D");
        assert_eq!(range.get(b"St").unwrap().as_i64().unwrap(), 1);
        // Pages 1 et 3 : les trois liens de la page 1 visent leur page copiée.
        let mut dst = raw_copy(bytes, &[0, 2]);
        carry_catalog(&src, &mut dst, &[0, 2], 0, None);
        assert!(!has_key_path(&dst, &["OCProperties"]), "calque non utilisé");
        let ids = pages(&dst);
        let page = dst.get_dictionary(ids[0]).unwrap();
        let mut targets = Vec::new();
        for link in page.get(b"Annots").unwrap().as_array().unwrap() {
            let link = dict(&dst, link);
            if link.get(b"Subtype").unwrap().as_name().unwrap() != b"Link" {
                continue;
            }
            let dest = link.get(b"Dest").ok().or_else(|| {
                let action = dict(&dst, link.get(b"A").ok()?);
                action.get(b"D").ok()
            });
            targets.extend(dest.cloned());
        }
        // chap2 (page 2) n'est pas copiée : ce lien perd sa cible ; les deux autres visent p3,
        // en destination explicite (PDFium avait retiré celle du troisième).
        assert_eq!(targets.len(), 2, "{targets:?}");
        let explicit: Vec<&Object> = targets
            .iter()
            .filter_map(|d| d.as_array().ok())
            .map(|a| &a[0])
            .collect();
        assert_eq!(explicit, [&Object::Reference(ids[1]); 2], "{targets:?}");
    }

    /// `rich_catalog_pdf` dont le calque est **masqué** à l'ouverture (`/BaseState /OFF`).
    fn rich_with_hidden_layer() -> Vec<u8> {
        let mut doc = lo(&testing::rich_catalog_pdf("1.7", None));
        let ocg = Object::Reference((12, 0));
        let catalog = doc.catalog_mut().unwrap();
        let properties = catalog
            .get_mut(b"OCProperties")
            .unwrap()
            .as_dict_mut()
            .unwrap();
        properties.set(
            "D",
            dictionary! { "BaseState" => Object::Name(b"OFF".to_vec()), "Order" => vec![ocg] },
        );
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn a_hidden_layer_stays_hidden_after_extraction() {
        let bytes = rich_with_hidden_layer();
        let src = lo(&bytes);
        let mut dst = raw_copy(bytes, &[1]);
        carry_catalog(&src, &mut dst, &[1], 0, None);
        let properties = dict(&dst, dst.catalog().unwrap().get(b"OCProperties").unwrap());
        let ocgs = properties.get(b"OCGs").unwrap().as_array().unwrap();
        assert_eq!(ocgs.len(), 1);
        let config = dict(&dst, properties.get(b"D").unwrap());
        assert_eq!(config.get(b"OFF").unwrap().as_array().unwrap(), ocgs);
        assert!(!config.has(b"ON"), "calque masqué dans la source");
        assert_eq!(config.get(b"Order").unwrap().as_array().unwrap(), ocgs);
    }

    /// (première page, style, premier numéro) de chaque plage de `/PageLabels`.
    fn labels(doc: &LoDocument) -> Vec<(i64, String, i64)> {
        let tree = dict(doc, doc.catalog().unwrap().get(b"PageLabels").unwrap());
        let nums = tree.get(b"Nums").unwrap().as_array().unwrap();
        nums.chunks(2)
            .map(|pair| {
                let style = dict(doc, &pair[1]);
                (
                    pair[0].as_i64().unwrap(),
                    String::from_utf8_lossy(style.get(b"S").unwrap().as_name().unwrap())
                        .into_owned(),
                    style.get(b"St").unwrap().as_i64().unwrap(),
                )
            })
            .collect()
    }

    #[test]
    fn page_labels_follow_each_source_on_merge() {
        let (rich, plain) = (testing::rich_catalog_pdf("1.7", None), simple());
        let (src_rich, src_plain) = (lo(&rich), lo(&plain));
        let n = src_plain.get_pages().len();
        let plain_pages: Vec<u16> = (0..n as u16).collect();
        let at = |i: usize| i as i64;
        // Simple puis riche : les pages de simple gardent leur numéro, puis les plages du riche.
        let mut dst = raw_merge(&[plain.clone(), rich.clone()]);
        carry_catalog(&src_plain, &mut dst, &plain_pages, 0, None);
        carry_catalog(&src_rich, &mut dst, &[0, 1, 2], n, Some(" (2)"));
        assert_eq!(
            labels(&dst),
            [
                (0, "D".into(), 1),
                (at(n), "r".into(), 1),
                (at(n + 1), "D".into(), 1)
            ]
        );
        // Riche puis simple : les pages de simple sont numérotées selon leur position.
        let mut dst = raw_merge(&[rich, plain]);
        carry_catalog(&src_rich, &mut dst, &[0, 1, 2], 0, None);
        carry_catalog(&src_plain, &mut dst, &plain_pages, 3, Some(" (2)"));
        assert_eq!(
            labels(&dst),
            [(0, "r".into(), 1), (1, "D".into(), 1), (3, "D".into(), 4)]
        );
    }

    /// `form_parent_kid_pdf` dont le champ **et** le widget portent un script `/AA`.
    fn form_with_scripts() -> Vec<u8> {
        let mut doc = lo(&testing::form_parent_kid_pdf());
        let script = || {
            Object::Dictionary(dictionary! {
                "K" => dictionary! { "S" => Object::Name(b"JavaScript".to_vec()),
                                     "JS" => Object::string_literal("app.alert(1);") }
            })
        };
        for id in [(5, 0), (6, 0)] {
            doc.get_dictionary_mut(id).unwrap().set("AA", script());
        }
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn field_and_widget_scripts_are_not_carried() {
        let bytes = form_with_scripts();
        let src = lo(&bytes);
        let mut dst = raw_copy(bytes, &[0]);
        carry_catalog(&src, &mut dst, &[0], 0, None);
        assert_eq!(top_level_names(&dst), ["nom"]);
        for object in dst.objects.values() {
            let Ok(dict) = object.as_dict() else {
                continue;
            };
            let widget = dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Widget");
            let field = dict.has(b"FT") || dict.has(b"T");
            assert!(
                !((widget || field) && dict.has(b"AA")),
                "script reporté : {dict:?}"
            );
        }
    }

    #[test]
    fn hostile_page_label_start_does_not_overflow() {
        let mut doc = lo(&testing::rich_catalog_pdf("1.7", None));
        let nums = vec![
            Object::Integer(i64::MIN),
            Object::Dictionary(
                dictionary! { "S" => Object::Name(b"D".to_vec()), "St" => i64::MAX },
            ),
        ];
        doc.catalog_mut()
            .unwrap()
            .set("PageLabels", dictionary! { "Nums" => nums });
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        let src = lo(&bytes);
        let mut dst = raw_copy(bytes, &[2]);
        carry_catalog(&src, &mut dst, &[2], 0, None);
        assert_eq!(labels(&dst), [(0, "D".into(), i64::MAX)]);
    }

    #[test]
    fn a_page_extracted_twice_keeps_distinct_annotations() {
        let bytes = testing::rich_catalog_pdf("1.7", None);
        let src = lo(&bytes);
        let mut dst = raw_copy(bytes, &[0, 0]);
        carry_catalog(&src, &mut dst, &[0, 0], 0, None);
        let annots = |page: ObjectId| -> Vec<Object> {
            dst.get_dictionary(page)
                .unwrap()
                .get(b"Annots")
                .unwrap()
                .as_array()
                .unwrap()
                .clone()
        };
        let ids = pages(&dst);
        let (first, second) = (annots(ids[0]), annots(ids[1]));
        assert_eq!(first.len(), 4);
        assert!(
            first.iter().all(|a| !second.contains(a)),
            "annotations partagées : {first:?} / {second:?}"
        );
    }

    #[test]
    fn named_destinations_follow_their_copied_page() {
        let bytes = testing::rich_catalog_pdf("1.7", None);
        let src = lo(&bytes);
        let mut dst = raw_copy(bytes, &[1]);
        carry_catalog(&src, &mut dst, &[1], 0, None);
        let page = Object::Reference(pages(&dst)[0]);
        let names = dict(&dst, dst.catalog().unwrap().get(b"Names").unwrap());
        let tree = dict(&dst, names.get(b"Dests").unwrap());
        let entries = tree.get(b"Names").unwrap().as_array().unwrap();
        assert_eq!(entries.len(), 2, "{entries:?}");
        assert_eq!(text(&entries[0]), "chap2");
        assert_eq!(entries[1].as_array().unwrap()[0], page);
        // `/chap3` mène à la page 3, non copiée : pas de `/Dests` du tout.
        assert!(!has_key_path(&dst, &["Dests"]));
        let mut dst = raw_copy(testing::rich_catalog_pdf("1.7", None), &[2]);
        carry_catalog(&src, &mut dst, &[2], 0, None);
        let dests = dict(&dst, dst.catalog().unwrap().get(b"Dests").unwrap());
        assert_eq!(dests.len(), 1);
        let chap3 = dests.get(b"chap3").unwrap().as_array().unwrap();
        assert_eq!(chap3[0], Object::Reference(pages(&dst)[0]));
        assert!(!has_key_path(&dst, &["Names", "Dests"]), "chap2 non copiée");
    }
}
