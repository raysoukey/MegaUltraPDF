// SPDX-License-Identifier: GPL-3.0-or-later
//! Passe `lopdf` appliquée aux octets **avant l'écriture** d'un fichier (ADR-0010).
//!
//! PDFium laisse des traces que les autres lecteurs ne devraient pas voir : une clé privée
//! `/PDFIUM_HasGeneratedAP` et une apparence imposée aux notes (icône jaune fixe), des
//! objets orphelins créés à chaque rendu, et nos formes `Ink` n'ont pas l'`/InkList` que la
//! norme exige (ADR-0008). On corrige tout cela sur l'arbre d'objets, sans toucher au document
//! PDFium en mémoire.
//!
//! \[Apprentissage\] **Flux de contenu** : l'apparence d'une annotation est un petit programme
//! (`m` déplacer, `l` ligne, `c` courbe, `re` rectangle…). `lopdf::content::Content::decode`
//! le découpe en opérations ; on en déduit la liste de points attendue dans `/InkList`.

use std::collections::BTreeSet;

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::error::{CoreError, CoreResult};

mod mcid;
mod subset;

fn structure(e: impl std::fmt::Display) -> CoreError {
    CoreError::InvalidOperation(format!("nettoyage avant enregistrement : {e}"))
}

/// Résultat de [`before_save`] : les octets à écrire, et si le nettoyage a pu s'appliquer.
///
/// Pourquoi : quand `lopdf` ne relit pas le fichier, `before_save` rend les octets de PDFium
/// tels quels, orphelins compris ; l'appelant qui vient de supprimer quelque chose doit le
/// savoir pour refuser d'écrire un fichier où la suppression survivrait (audit E1). Tout
/// appelant qui **écrit** ces octets passe par [`Saved::into_checked`], qui fait cette
/// vérification : c'est la règle du crate, et la seule voie offerte aux autres crates.
///
/// Les champs restent publics pour les tests d'intégration (`tests/`, hors du crate), qui
/// examinent justement le repli (`cleaned` faux, `bytes` tels quels) ; ni le CLI ni
/// l'application ne les lisent (relecture finale du Lot 14, M4).
///
/// \[Apprentissage\] `#[must_use]` : le compilateur avertit si une valeur de ce type est
/// ignorée (calculée puis jetée sans être lue), ce qui serait ici un nettoyage oublié.
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    /// Les octets prêts à écrire. Lus directement par les seuls tests : pour écrire, passer
    /// par [`Saved::into_checked`].
    pub bytes: Vec<u8>,
    /// Faux si `before_save` a dû se rabattre sur les octets de PDFium sans nettoyage.
    pub cleaned: bool,
}

impl Saved {
    /// Les octets à écrire, après `refuse_if_uncleaned` : refusés si une suppression a eu
    /// lieu (`needs_cleanup`) et que le nettoyage n'a pas pu s'appliquer. `what` ouvre le
    /// message d'erreur (« enregistrement refusé »…).
    ///
    /// Pourquoi : un seul chemin des octets vers le disque pour tous les appelants de
    /// production, dans ce crate comme ailleurs ; lire `bytes` directement permettrait
    /// d'oublier la vérification.
    pub fn into_checked(self, needs_cleanup: bool, what: &str) -> CoreResult<Vec<u8>> {
        refuse_if_uncleaned(needs_cleanup, &self, what)?;
        Ok(self.bytes)
    }
}

/// Octets prêts à écrire : notes sans apparence imposée, `/InkList` sur les formes, objets
/// orphelins élagués et renumérotés. Un document chiffré (ouvert avec `password`) est nettoyé
/// en clair puis **rechiffré à l'identique** (`crypt::LopdfSession`, Lot 10). Un document
/// à filtres `Identity` est relu en **passe-plat** (rien à déchiffrer) : élagué, mais **pas
/// renuméroté**, car le numéro d'un objet entre dans la clé de ses octets chiffrés (Lot 14).
///
/// `min_version` est la version d'en-tête du document d'origine : PDFium ne sait écrire que
/// 1.0 à 1.7 (un `%PDF-2.0` ressort en `%PDF-1.0`), et l'en-tête écrit est le plus grand de
/// celui des octets et de `min_version` (audit E2). `None` si aucune version n'est connue.
pub fn before_save(
    bytes: &[u8],
    password: Option<&str>,
    min_version: Option<(u8, u8)>,
) -> CoreResult<Saved> {
    before_save_with(bytes, password, min_version, CleanupOptions::default())
}

/// Réglages facultatifs de [`before_save_with`].
///
/// Pourquoi : retirer `/NeedAppearances` n'est sûr que si les `/AP` écrits sont à jour. Seul
/// l'enregistrement du document ouvert le sait (il vient de les faire régénérer par PDFium) ;
/// l'extraction, la fusion et la protection partent d'octets dont les `/AP` peuvent dater
/// d'avant la dernière valeur de champ : elles gardent la clé.
///
/// \[Apprentissage\] Une **structure d'options** plutôt qu'un paramètre `bool` de plus : à
/// l'appel, `CleanupOptions { appearances_fresh: true, ..Default::default() }` nomme chaque
/// réglage (un `true` nu ne dit rien), et `..Default::default()` remplit les champs qu'on ne
/// cite pas avec leur valeur par défaut (`#[derive(Default)]` : `false` pour un `bool`). Un
/// réglage ajouté plus tard ne change donc aucun appel existant.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CleanupOptions {
    /// Les apparences des widgets viennent d'être régénérées : `/NeedAppearances` peut partir.
    pub appearances_fresh: bool,
}

/// [`before_save`] avec des réglages (voir [`CleanupOptions`]).
pub(crate) fn before_save_with(
    bytes: &[u8],
    password: Option<&str>,
    min_version: Option<(u8, u8)>,
    opts: CleanupOptions,
) -> CoreResult<Saved> {
    let raw = || Saved {
        bytes: crate::crypt::raise_header_version(bytes, min_version),
        cleaned: false,
    };
    let mut session = match crate::crypt::LopdfSession::load(bytes, password) {
        Ok(session) => session,
        // Un fichier que lopdf ne relit (ou ne déchiffre) pas est écrit tel que PDFium l'a
        // produit, protection comprise.
        Err(_) => return Ok(raw()),
    };
    let keeps_numbers = session.keeps_object_numbers();
    let doc = &mut session.doc;
    // [Apprentissage] `Option` est ordonnée : `None` est plus petit que tout `Some(_)`, et deux
    // `Some` se comparent par leur contenu. `max` garde donc la version connue, ou la plus
    // récente des deux (comparaison des tuples : voir `crypt::parse_version`).
    let written = crate::crypt::header_version(bytes).max(min_version);
    if let Some((major, minor)) = written {
        doc.version = format!("{major}.{minor}");
    }
    let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
    // Le nettoyage ne touche à aucun flux de contenu : on en garde l'empreinte pour vérifier
    // la relecture (un rechiffrement faux brouillerait le contenu sans changer les pages).
    let contents_before = page_contents(doc);
    for page_id in &pages {
        clean_page_annotations(doc, *page_id)?;
        identify_annotations(doc, *page_id)?;
    }
    // Avant le retrait des liens pendants : un lien retiré ici y perd aussi popup et `/OBJR`.
    neutralize_dead_destinations(doc);
    drop_dangling_annotation_links(doc, &pages)?;
    // Un objet supprimé ne doit plus être cité par l'arbre de structure (Lot 14, E5) ; sans
    // `/StructTreeRoot`, la passe ne fait rien.
    mcid::prune_orphan_mcids(doc);
    dedupe_font_files(doc);
    // Après la déduplication : un programme partagé donne un seul sous-ensemble (audit E10).
    subset::subset_embedded_fonts(doc);
    if opts.appearances_fresh {
        drop_need_appearances(doc);
    }
    // Le fichier est réécrit : sa date de modification aussi (C12, Lot 9).
    crate::properties::stamp_mod_date(doc);
    let pages_before = doc.get_pages().len();
    prune(doc);
    // Passe-plat : le numéro d'un objet entre dans la clé de ses octets chiffrés.
    if !keeps_numbers {
        doc.renumber_objects();
    }
    let out = session.finish()?;
    // Ceinture de sécurité : la réécriture `lopdf` remplace le fichier de l'utilisateur ; si
    // elle ne se relit (et ne se déchiffre) pas avec le même nombre de pages et les mêmes
    // contenus, on écrit les octets PDFium tels quels. Une seule relecture pour les deux
    // contrôles (audit du Lot 13, R6 : trois chargements par enregistrement auparavant).
    let reread = reload_if_sane(pages_before, &out, password).map(|s| page_contents(&s.doc));
    if reread.as_ref() != Some(&contents_before) {
        return Ok(raw());
    }
    Ok(Saved {
        bytes: out,
        cleaned: true,
    })
}

/// Ce que PDFium doit dessiner avant l'enregistrement pour que chaque annotation ait son
/// apparence (ISO 32000-2 §12.5.5, PDF/A-2 6.3.3) : calculé par **une** passe `lopdf`, pour ne
/// charger dans PDFium que les pages concernées (auparavant, toutes les pages à chaque
/// enregistrement : coûteux sur un long document).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SavePlan {
    /// L'`/AcroForm` porte `/NeedAppearances true` : les widgets sont à régénérer.
    pub need_appearances: bool,
    /// Index (à partir de 0, croissants) des pages à rendre : un marquage de texte
    /// (`Highlight`, `Underline`, `StrikeOut`, `Squiggly`) sans `/AP /N`, ou un widget quand
    /// `need_appearances` est vrai ou qu'il n'a pas d'apparence utilisable.
    pub pages: Vec<u16>,
}

/// Le [`SavePlan`] des `bytes` (écrits par PDFium) ; `None` si `lopdf` ne sait pas les lire :
/// l'appelant ne rend alors rien et garde `/NeedAppearances` (rien ne prouve que les
/// apparences sont à jour). Sans `/Annots` dans les octets, plan vide sans seconde lecture
/// (PDFium n'écrit pas de flux d'objets : les dictionnaires de page y sont en clair).
pub(crate) fn save_plan(bytes: &[u8], password: Option<&str>) -> Option<SavePlan> {
    if !bytes.windows(7).any(|w| w == b"/Annots") {
        return Some(SavePlan::default());
    }
    let session = crate::crypt::LopdfSession::load(bytes, password).ok()?;
    let doc = &session.doc;
    let need_appearances = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"AcroForm").ok())
        .and_then(|a| doc.dereference(a).ok())
        .and_then(|(_, a)| {
            a.as_dict()
                .ok()?
                .get(b"NeedAppearances")
                .ok()?
                .as_bool()
                .ok()
        })
        .unwrap_or(false);
    let pages = doc
        .get_pages()
        .into_iter()
        .filter(|(_, page_id)| {
            page_annots(doc, *page_id).is_some_and(|(_, annots)| {
                annots.iter().any(|a| {
                    doc.dereference(a)
                        .ok()
                        .and_then(|(_, a)| a.as_dict().ok())
                        .is_some_and(|a| needs_rendering(doc, a, need_appearances))
                })
            })
        })
        // Numéros de page `lopdf` : 1, 2, 3… ; un numéro hors de `u16` n'est pas une page que
        // PDFium accepterait (`from_bytes` refuse plus de `u16::MAX` pages).
        .filter_map(|(number, _)| u16::try_from(number.checked_sub(1)?).ok())
        .collect();
    Some(SavePlan {
        need_appearances,
        pages,
    })
}

/// Vrai si l'annotation `annot` demande un rendu PDFium avant l'enregistrement (voir
/// [`SavePlan::pages`]).
fn needs_rendering(doc: &Document, annot: &Dictionary, need_appearances: bool) -> bool {
    match annot.get(b"Subtype").and_then(Object::as_name) {
        Ok(b"Highlight" | b"Underline" | b"StrikeOut" | b"Squiggly") => {
            let normal = annot
                .get(b"AP")
                .ok()
                .and_then(|ap| doc.dereference(ap).ok())
                .and_then(|(_, ap)| ap.as_dict().ok())
                .is_some_and(|ap| ap.has(b"N"));
            !normal
        }
        Ok(b"Widget") => need_appearances || !has_normal_appearance(doc, annot),
        _ => false,
    }
}

/// Retire `/NeedAppearances true` de l'`/AcroForm` quand **chaque** widget de chaque page a une
/// apparence `/AP /N` utilisable (un flux, ou un sous-dictionnaire qui contient l'état `/AS`
/// du widget). PDFium régénère les apparences des champs remplis ; la clé, elle, resterait :
/// PDF/A-2 l'interdit (6.4.1) et Acrobat propose d'enregistrer à chaque ouverture (audit E7).
/// Un seul widget sans apparence suffit à la garder : elle est alors ce qui le fait dessiner.
pub(crate) fn drop_need_appearances(doc: &mut Document) {
    let Some(holder) = acroform_holder(doc) else {
        return;
    };
    let needs = match &holder {
        AcroForm::Indirect(id) => doc.get_dictionary(*id).ok(),
        AcroForm::Direct => doc
            .catalog()
            .ok()
            .and_then(|c| c.get(b"AcroForm").ok())
            .and_then(|a| a.as_dict().ok()),
    }
    .and_then(|d| d.get(b"NeedAppearances").ok())
    .and_then(|v| v.as_bool().ok());
    if needs != Some(true) {
        return;
    }
    let all_drawn = doc.get_pages().into_values().all(|page| {
        let Some((_, annots)) = page_annots(doc, page) else {
            return true;
        };
        annots.iter().all(|a| {
            let Ok((_, obj)) = doc.dereference(a) else {
                return true;
            };
            let Ok(dict) = obj.as_dict() else {
                return true;
            };
            dict.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Widget")
                || has_normal_appearance(doc, dict)
        })
    });
    if !all_drawn {
        return;
    }
    match holder {
        AcroForm::Indirect(id) => {
            if let Ok(d) = doc.get_dictionary_mut(id) {
                d.remove(b"NeedAppearances");
            }
        }
        AcroForm::Direct => {
            if let Ok(Object::Dictionary(d)) =
                doc.catalog_mut().and_then(|c| c.get_mut(b"AcroForm"))
            {
                d.remove(b"NeedAppearances");
            }
        }
    }
}

/// Où vit le dictionnaire `/AcroForm` : objet indirect, ou direct dans le catalogue.
enum AcroForm {
    Indirect(ObjectId),
    Direct,
}

fn acroform_holder(doc: &Document) -> Option<AcroForm> {
    match doc.catalog().ok()?.get(b"AcroForm").ok()? {
        Object::Reference(id) => Some(AcroForm::Indirect(*id)),
        Object::Dictionary(_) => Some(AcroForm::Direct),
        _ => None,
    }
}

/// Vrai si `/AP /N` du widget est un flux, ou un sous-dictionnaire qui contient son état `/AS`.
fn has_normal_appearance(doc: &Document, widget: &Dictionary) -> bool {
    let Some(ap) = widget
        .get(b"AP")
        .ok()
        .and_then(|a| doc.dereference(a).ok())
        .and_then(|(_, a)| a.as_dict().ok())
    else {
        return false;
    };
    let Some((_, normal)) = ap.get(b"N").ok().and_then(|n| doc.dereference(n).ok()) else {
        return false;
    };
    match normal {
        Object::Stream(_) => true,
        Object::Dictionary(states) => widget
            .get(b"AS")
            .and_then(Object::as_name)
            .is_ok_and(|state| states.has(state)),
        _ => false,
    }
}

/// Refuse d'écrire `saved` quand une suppression a eu lieu (`needs_cleanup`) et que le
/// nettoyage n'a pas pu s'appliquer : les restes de ce qui a été supprimé survivraient (E1).
/// `what` ouvre le message (« enregistrement refusé », « protection refusée »…).
fn refuse_if_uncleaned(needs_cleanup: bool, saved: &Saved, what: &str) -> CoreResult<()> {
    if needs_cleanup && !saved.cleaned {
        return Err(CoreError::InvalidOperation(format!(
            "{what} : le nettoyage n'a pas pu s'appliquer après une suppression ; \
             le fichier d'origine est conservé"
        )));
    }
    Ok(())
}

/// Vrai si [`before_save`] pourra nettoyer ces octets : `lopdf` doit les relire (et les
/// déchiffrer avec `password`).
///
/// Pourquoi : `FPDF_SaveAsCopy` recopie tous les objets, y compris ceux qu'une suppression a
/// rendus orphelins (le texte d'une annotation supprimée resterait en clair dans le fichier,
/// audit E1) ; seul l'élagage `lopdf` de `before_save` les retire. Quand `lopdf` ne relit pas
/// le fichier (filtres `Identity`, structure qu'il refuse), `before_save` écrit les octets de
/// PDFium tels quels : on refuse alors les suppressions (`Document::ensure_cleanable`).
///
/// Exposée uniquement pour les tests d'intégration, comme [`before_save`].
#[doc(hidden)]
pub fn can_clean(bytes: &[u8], password: Option<&str>) -> bool {
    check_cleanable(bytes, password).is_ok()
}

/// Comme [`can_clean`], avec la raison du refus (`Document::ensure_cleanable` la transmet à
/// l'utilisateur, ADR-0005).
pub(crate) fn check_cleanable(bytes: &[u8], password: Option<&str>) -> CoreResult<()> {
    crate::crypt::LopdfSession::load(bytes, password).map(drop)
}

/// Octets **bruts** (encore compressés, déjà déchiffrés) des flux de contenu de chaque page,
/// dans l'ordre des pages.
///
/// Le nettoyage ne recompresse ni ne réécrit aucun flux de contenu : leurs octets bruts
/// doivent se retrouver à l'identique dans la réécriture. Les comparer sans les décompresser
/// ne coûte que la taille du fichier, même devant une bombe de décompression répartie sur
/// plusieurs pages, et laisse le nettoyage s'appliquer à ce fichier (relecture de la PR A du
/// Lot 13 ; auparavant, le contenu décodé de chaque page était gardé deux fois en mémoire).
fn page_contents(doc: &Document) -> Vec<Vec<u8>> {
    doc.get_pages()
        .into_values()
        .map(|page| {
            let mut raw = Vec::new();
            for id in doc.get_page_contents(page) {
                if let Ok(stream) = doc.get_object(id).and_then(Object::as_stream) {
                    raw.extend_from_slice(&stream.content);
                }
                raw.push(b'\n');
            }
            raw
        })
        .collect()
}

/// Objets atteignables depuis la remorque (racine, `/Info`, `/Encrypt`…), en suivant toutes
/// les références des dictionnaires, tableaux et dictionnaires de flux.
///
/// \[Apprentissage\] `lopdf::Document::prune_objects` tient la liste des objets vus dans
/// un `Vec` et appelle `contains` à chaque référence : temps **quadratique**, sensible sur un
/// gros fichier (C22, Lot 9). Un `BTreeSet` répond à « déjà vu ? » en temps logarithmique.
pub(crate) fn reachable(doc: &Document) -> BTreeSet<ObjectId> {
    fn refs_of(object: &Object, out: &mut Vec<ObjectId>) {
        match object {
            Object::Reference(id) => out.push(*id),
            Object::Array(items) => items.iter().for_each(|o| refs_of(o, out)),
            Object::Dictionary(dict) => dict.iter().for_each(|(_, o)| refs_of(o, out)),
            Object::Stream(stream) => stream.dict.iter().for_each(|(_, o)| refs_of(o, out)),
            _ => {}
        }
    }
    let mut seen = BTreeSet::new();
    let mut todo = Vec::new();
    for (_, value) in doc.trailer.iter() {
        refs_of(value, &mut todo);
    }
    while let Some(id) = todo.pop() {
        if seen.insert(id)
            && let Some(object) = doc.objects.get(&id)
        {
            refs_of(object, &mut todo);
        }
    }
    seen
}

/// Fait pointer les descripteurs de police vers **un seul** exemplaire de chaque programme
/// de police identique (`/FontFile`, `/FontFile2`, `/FontFile3`) ; l'élagage retire ensuite
/// les copies. Rend le nombre de références réécrites.
///
/// PDFium incorpore la police Unicode **entière** à chaque chargement (Lot 10) : après un
/// rechargement du document (filigrane, annuler…), une nouvelle insertion de texte en
/// recharge une copie. Sans cette passe, chaque copie ajouterait environ 400 Ko au fichier.
pub(crate) fn dedupe_font_files(doc: &mut Document) -> usize {
    use std::collections::BTreeMap;
    const KEYS: [&[u8]; 3] = [b"FontFile", b"FontFile2", b"FontFile3"];
    // Programme de police (filtre + octets) → premier objet qui le porte.
    let mut first: BTreeMap<(Vec<u8>, Vec<u8>), ObjectId> = BTreeMap::new();
    let mut rewrites: Vec<(ObjectId, &'static [u8], ObjectId)> = Vec::new();
    for (id, object) in &doc.objects {
        let Ok(dict) = object.as_dict() else {
            continue;
        };
        if dict.get(b"Type").ok() != Some(&Object::Name(b"FontDescriptor".to_vec())) {
            continue;
        }
        for key in KEYS {
            let Ok(file) = dict.get(key).and_then(Object::as_reference) else {
                continue;
            };
            let Some(Ok(stream)) = doc.objects.get(&file).map(Object::as_stream) else {
                continue;
            };
            let filter = stream
                .dict
                .get(b"Filter")
                .ok()
                .map(|f| format!("{f:?}").into_bytes())
                .unwrap_or_default();
            let canonical = *first
                .entry((filter, stream.content.clone()))
                .or_insert(file);
            if canonical != file {
                rewrites.push((*id, key, canonical));
            }
        }
    }
    for (descriptor, key, canonical) in &rewrites {
        if let Ok(dict) = doc.get_dictionary_mut(*descriptor) {
            dict.set(*key, Object::Reference(*canonical));
        }
    }
    rewrites.len()
}

/// Retire les objets inatteignables (orphelins) et rend leur nombre.
pub(crate) fn prune(doc: &mut Document) -> usize {
    let keep = reachable(doc);
    let before = doc.objects.len();
    doc.objects.retain(|id, _| keep.contains(id));
    before - doc.objects.len()
}

/// Vrai si `out` se relit (et, s'il est chiffré, se déchiffre avec `password`) comme un PDF
/// non vide ayant encore `pages_before` pages.
pub(crate) fn rewrite_is_sane(pages_before: usize, out: &[u8], password: Option<&str>) -> bool {
    reload_if_sane(pages_before, out, password).is_some()
}

/// `out` relu (et déchiffré), s'il passe le contrôle de [`rewrite_is_sane`].
fn reload_if_sane(
    pages_before: usize,
    out: &[u8],
    password: Option<&str>,
) -> Option<crate::crypt::LopdfSession> {
    let reloaded = crate::crypt::LopdfSession::load_exact(out, password).ok()?;
    (reloaded.doc.get_pages().len() == pages_before && reloaded.doc.trailer.get(b"Root").is_ok())
        .then_some(reloaded)
}

/// Nettoie chaque annotation d'une page. PDFium écrit une annotation fraîchement créée
/// **directement** dans le tableau `/Annots` de la page, et sous forme de référence après
/// un rendu : les deux cas sont traités, et le tableau n'est réécrit que s'il a changé.
fn clean_page_annotations(doc: &mut Document, page_id: ObjectId) -> CoreResult<()> {
    let annots = doc
        .get_dictionary(page_id)
        .ok()
        .and_then(|page| page.get(b"Annots").ok().cloned());
    let Some(annots) = annots else {
        return Ok(());
    };
    // Le tableau peut lui-même être un objet indirect : on retient où le réécrire.
    let (array_id, array) = match &annots {
        Object::Reference(id) => (
            Some(*id),
            doc.get_object(*id)
                .ok()
                .and_then(|o| o.as_array().ok())
                .cloned(),
        ),
        Object::Array(array) => (None, Some(array.clone())),
        _ => (None, None),
    };
    let Some(mut array) = array else {
        return Ok(());
    };
    let mut changed = false;
    for item in array.iter_mut() {
        match item {
            Object::Reference(id) => {
                let id = *id;
                let cleaned = doc.get_dictionary(id).ok().and_then(|d| cleaned(doc, d));
                if let Some(new_dict) = cleaned {
                    *doc.get_dictionary_mut(id).map_err(structure)? = new_dict;
                }
            }
            Object::Dictionary(dict) => {
                if let Some(new_dict) = cleaned(doc, dict) {
                    *dict = new_dict;
                    changed = true;
                }
            }
            _ => {}
        }
    }
    if changed {
        match array_id {
            Some(id) => {
                doc.objects.insert(id, Object::Array(array));
            }
            None => doc
                .get_dictionary_mut(page_id)
                .map_err(structure)?
                .set("Annots", Object::Array(array)),
        }
    }
    Ok(())
}

/// Le tableau `/Annots` d'une page : où le réécrire (objet indirect, ou `None` s'il est
/// direct dans la page) et ses éléments.
fn page_annots(doc: &Document, page_id: ObjectId) -> Option<(Option<ObjectId>, Vec<Object>)> {
    match doc.get_dictionary(page_id).ok()?.get(b"Annots").ok()? {
        Object::Reference(id) => Some((
            Some(*id),
            doc.get_object(*id).ok()?.as_array().ok()?.clone(),
        )),
        Object::Array(array) => Some((None, array.clone())),
        _ => None,
    }
}

/// Ne garde, dans le `/Annots` de la page, que les annotations pour lesquelles `keep` est vrai
/// (un élément qui n'est pas un dictionnaire lisible est gardé) ; le tableau, direct ou
/// indirect, n'est réécrit que s'il a changé.
///
/// \[Apprentissage\] `keep` reçoit le document en **lecture** : la fermeture ne peut pas
/// capturer `doc`, que cette fonction emprunte déjà en écriture.
fn retain_annots(
    doc: &mut Document,
    page_id: ObjectId,
    mut keep: impl FnMut(&Document, &Dictionary) -> bool,
) -> CoreResult<()> {
    let Some((array_id, items)) = page_annots(doc, page_id) else {
        return Ok(());
    };
    let kept: Vec<Object> = items
        .iter()
        .filter(|item| {
            let dict = match item {
                Object::Reference(id) => doc.get_dictionary(*id).ok(),
                Object::Dictionary(dict) => Some(dict),
                _ => None,
            };
            dict.is_none_or(|d| keep(doc, d))
        })
        .cloned()
        .collect();
    if kept.len() == items.len() {
        return Ok(());
    }
    match array_id {
        Some(id) => {
            doc.objects.insert(id, Object::Array(kept));
        }
        None => doc
            .get_dictionary_mut(page_id)
            .map_err(structure)?
            .set("Annots", Object::Array(kept)),
    }
    Ok(())
}

/// Donne à chaque annotation de la page qui n'en a pas un `/NM` (nom unique sur la page)
/// et un `/P` (sa page) : clés facultatives que PDFium n'écrit pas, mais dont se servent les
/// lecteurs qui synchronisent, fusionnent ou répondent à des commentaires (§12.5.2, B3).
fn identify_annotations(doc: &mut Document, page_id: ObjectId) -> CoreResult<()> {
    let Some((array_id, mut items)) = page_annots(doc, page_id) else {
        return Ok(());
    };
    let mut names: BTreeSet<Vec<u8>> = items
        .iter()
        .filter_map(|item| match item {
            Object::Reference(id) => doc.get_dictionary(*id).ok(),
            Object::Dictionary(dict) => Some(dict),
            _ => None,
        })
        .filter_map(|d| {
            d.get(b"NM")
                .and_then(Object::as_str)
                .ok()
                .map(<[u8]>::to_vec)
        })
        .collect();
    let mut inline_changed = false;
    for (i, item) in items.iter_mut().enumerate() {
        let mut fresh = || {
            let mut name = format!("mega-{}-{}-{i}", page_id.0, page_id.1).into_bytes();
            while names.contains(&name) {
                name.push(b'x');
            }
            names.insert(name.clone());
            Object::String(name, lopdf::StringFormat::Literal)
        };
        let dict = match item {
            Object::Reference(id) => match doc.get_dictionary_mut(*id) {
                Ok(dict) => dict,
                Err(_) => continue,
            },
            Object::Dictionary(dict) => {
                inline_changed |= !dict.has(b"NM") || !dict.has(b"P");
                dict
            }
            _ => continue,
        };
        if !dict.has(b"NM") {
            dict.set("NM", fresh());
        }
        if !dict.has(b"P") {
            dict.set("P", Object::Reference(page_id));
        }
    }
    if inline_changed {
        match array_id {
            Some(id) => {
                doc.objects.insert(id, Object::Array(items));
            }
            None => doc
                .get_dictionary_mut(page_id)
                .map_err(structure)?
                .set("Annots", Object::Array(items)),
        }
    }
    Ok(())
}

/// Une annotation retirée de la page (supprimée, caviardée) ne doit pas survivre dans le
/// fichier par un objet qui la cite encore (B20, Lot 9) : son **popup** (dont `/Parent` la
/// vise et qui, lui, peut être hors de la zone caviardée), une **réponse** (`/IRT`), ou
/// l'**arbre de structure** d'un PDF balisé (marqueur `/OBJR`). On retire ces liens ; l'élagage
/// qui suit supprime alors la note, son `/Contents` compris.
///
/// \[Apprentissage\] L'élagage garde tout objet **atteignable** depuis la racine : supprimer
/// une note de `/Annots` ne suffit pas tant qu'un autre objet la référence encore.
fn drop_dangling_annotation_links(doc: &mut Document, pages: &[ObjectId]) -> CoreResult<()> {
    // Annotations encore listées sur une page (par référence).
    let listed = |doc: &Document| -> BTreeSet<ObjectId> {
        pages
            .iter()
            .filter_map(|page| page_annots(doc, *page))
            .flat_map(|(_, items)| items)
            .filter_map(|item| item.as_reference().ok())
            .collect()
    };
    // 0. Membres d'un groupe (`/IRT` + `/RT /Group`, §12.5.6.2) dont l'annotation principale
    //    n'est plus listée : le groupe forme une seule annotation, ils partent avec elle.
    let primaries = listed(doc);
    for page_id in pages {
        retain_annots(doc, *page_id, |_, d| {
            let orphan_member = d.get(b"RT").and_then(Object::as_name).ok()
                == Some(b"Group".as_slice())
                && d.get(b"IRT")
                    .and_then(Object::as_reference)
                    .is_ok_and(|primary| !primaries.contains(&primary));
            !orphan_member
        })?;
    }
    let before = listed(doc);
    // 1. Popups dont le parent n'est plus listé : retirés de `/Annots`.
    for page_id in pages {
        retain_annots(doc, *page_id, |_, d| {
            let orphan = subtype(d) == b"Popup"
                && d.get(b"Parent")
                    .and_then(Object::as_reference)
                    .is_ok_and(|parent| !before.contains(&parent));
            !orphan
        })?;
    }
    // 2. Annotations qui ne sont plus listées nulle part.
    let alive = listed(doc);
    let dead: BTreeSet<ObjectId> = doc
        .objects
        .iter()
        .filter(|(id, object)| {
            !alive.contains(id)
                && object.as_dict().is_ok_and(|d| {
                    d.has(b"Rect")
                        && (d.get(b"Type").and_then(Object::as_name).ok()
                            == Some(b"Annot".as_slice())
                            || d.has(b"Subtype"))
                })
        })
        .map(|(id, _)| *id)
        .collect();
    if dead.is_empty() {
        return Ok(());
    }
    // 3. Leur texte d'abord, par précaution : même si un lien oublié les garde atteignables,
    //    plus rien de lisible n'y reste (relecture du Lot 9).
    for id in &dead {
        if let Ok(dict) = doc.get_dictionary_mut(*id) {
            for key in [
                b"Contents".as_slice(),
                b"RC",
                b"T",
                b"Subj",
                b"Popup",
                b"AP",
                b"FS",
            ] {
                dict.remove(key);
            }
        }
    }
    // 4. Marqueurs `/OBJR` vers elles rangés en objets indirects.
    let dead_objr: BTreeSet<ObjectId> = doc
        .objects
        .iter()
        .filter(|(_, o)| o.as_dict().is_ok_and(|d| is_objr_to(d, &dead)))
        .map(|(id, _)| *id)
        .collect();
    // 5. Liens `/IRT` et entrées `/OBJR` vers elles : retirés partout, objets directs compris.
    for object in doc.objects.values_mut() {
        scrub_links(object, &dead, &dead_objr, 0);
    }
    Ok(())
}

// --- Destinations vers une page supprimée (Lot 14, audit E4) -------------------------------

/// Profondeur suivie dans l'arbre des signets et l'arbre de noms `/Dests` (même borne que la
/// lecture des signets).
const MAX_DEPTH: usize = crate::outline::MAX_OUTLINE_DEPTH;

/// Retire du document les destinations qui visent une page absente de l'arbre des pages
/// (`get_pages`) : une page supprimée par PDFium devient un objet `null`, mais signets, liens
/// et destinations nommées la citaient encore (audit E4).
///
/// - entrées de `/Names /Dests` et du `/Dests` du catalogue vers une page morte : retirées ;
/// - signet (`/Outlines`) dont la `/Dest` ou l'action `/A /S /GoTo /D` vise une page morte,
///   directement ou par un nom retiré : il perd cette clé, mais reste (son titre et ses
///   enfants gardent leur sens) ;
/// - annotation de lien dans ce cas : retirée de `/Annots` (un lien sans cible ne mène nulle
///   part) ;
/// - `/OpenAction` du catalogue dans ce cas : retirée.
///
/// Pourquoi : un autre lecteur suivrait ces références vers un objet nul, et un objet encore
/// cité échapperait à l'élagage (`prune`) qui suit.
///
/// Critère d'une page morte : l'objet visé n'est pas une page de l'arbre que lit `lopdf`
/// (objet absent, objet qui n'est pas un dictionnaire de page, ou page détachée de l'arbre).
/// Ce critère suppose que `lopdf` voit les mêmes pages que PDFium : les octets nettoyés sont
/// ceux que PDFium vient d'écrire, et `move_pages` refuse déjà un document où les deux comptes
/// diffèrent. `reload_if_sane` vérifie ensuite que la réécriture garde le nombre de pages lu par
/// `lopdf`, mais un désaccord entre les deux bibliothèques n'est pas mesuré ici.
pub(crate) fn neutralize_dead_destinations(doc: &mut Document) {
    let live: BTreeSet<ObjectId> = doc.get_pages().into_values().collect();
    if live.is_empty() {
        return;
    }
    let dead_names = drop_dead_named_destinations(doc, &live);
    let dead = Dead {
        live: &live,
        names: &dead_names,
    };
    // Signets : clés retirées après coup (lecture puis écriture, pour le vérificateur
    // d'emprunts).
    let mut fixes: Vec<(ObjectId, &'static [u8])> = Vec::new();
    for item in outline_items(doc) {
        let Ok(dict) = doc.get_dictionary(item) else {
            continue;
        };
        if dict.get(b"Dest").is_ok_and(|d| dead.dest(doc, d)) {
            fixes.push((item, b"Dest"));
        }
        if dict.get(b"A").is_ok_and(|a| dead.goto(doc, a)) {
            fixes.push((item, b"A"));
        }
    }
    for (item, key) in fixes {
        if let Ok(dict) = doc.get_dictionary_mut(item) {
            dict.remove(key);
        }
    }
    // Liens des pages vivantes.
    for page_id in &live {
        // Ne peut échouer : la page vivante a déjà été lue par `get_pages`. Une erreur ne
        // laisserait que le lien, sans rien casser.
        let _ = retain_annots(doc, *page_id, |doc, d| !dead.link(doc, d));
    }
    // Action d'ouverture.
    let open_action_dead = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"OpenAction").ok())
        .is_some_and(|action| dead.dest(doc, action) || dead.goto(doc, action));
    if open_action_dead && let Ok(catalog) = doc.catalog_mut() {
        catalog.remove(b"OpenAction");
    }
}

/// Ce qui est mort : toute page hors de `live`, tout nom de `names`.
///
/// \[Apprentissage\] **Durée de vie nommée** : `Dead<'a>` ne possède pas ses ensembles, il les
/// emprunte. `'a` dit au compilateur qu'une valeur `Dead` ne doit pas vivre plus longtemps que
/// les ensembles qu'elle cite (voir `docs/apprentissage/concepts/ownership-et-emprunts.md`).
struct Dead<'a> {
    live: &'a BTreeSet<ObjectId>,
    names: &'a BTreeSet<Vec<u8>>,
}

impl Dead<'_> {
    /// Vrai si la destination `dest` (tableau explicite, `<< /D … >>` ou nom) mène à une page
    /// morte. Une destination vers une page désignée par son numéro (`GoToR`) n'est pas jugée.
    fn dest(&self, doc: &Document, dest: &Object) -> bool {
        let Ok((_, dest)) = doc.dereference(dest) else {
            return false;
        };
        let dest = match dest {
            Object::String(name, _) | Object::Name(name) => return self.names.contains(name),
            Object::Dictionary(dict) => match dict.get(b"D").and_then(|d| doc.dereference(d)) {
                Ok((_, d)) => d,
                Err(_) => return false,
            },
            other => other,
        };
        dest.as_array()
            .ok()
            .and_then(|items| items.first())
            .and_then(|page| page.as_reference().ok())
            .is_some_and(|page| !self.live.contains(&page))
    }

    /// Vrai si `action` est une action `/GoTo` dont la `/D` mène à une page morte.
    fn goto(&self, doc: &Document, action: &Object) -> bool {
        let Ok(Ok(action)) = doc.dereference(action).map(|(_, a)| a.as_dict()) else {
            return false;
        };
        action.get(b"S").and_then(Object::as_name).ok() == Some(b"GoTo".as_slice())
            && action.get(b"D").is_ok_and(|d| self.dest(doc, d))
    }

    /// Vrai si l'annotation `d` est un lien vers une page morte.
    fn link(&self, doc: &Document, d: &Dictionary) -> bool {
        subtype(d) == b"Link"
            && (d.get(b"Dest").is_ok_and(|dest| self.dest(doc, dest))
                || d.get(b"A").is_ok_and(|a| self.goto(doc, a)))
    }
}

/// Les signets de `/Outlines` (pile explicite, chacun visité une fois, profondeur bornée).
fn outline_items(doc: &Document) -> Vec<ObjectId> {
    let mut items = Vec::new();
    let root = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"Outlines").ok())
        .and_then(|o| doc.dereference(o).ok())
        .and_then(|(_, o)| o.as_dict().ok());
    let Some(first) = root.and_then(|r| r.get(b"First").and_then(Object::as_reference).ok()) else {
        return items;
    };
    let mut seen = BTreeSet::new();
    // (premier enfant d'une liste de frères, profondeur)
    let mut stack = vec![(first, 1)];
    while let Some((first, depth)) = stack.pop() {
        let mut next = Some(first);
        while let Some(id) = next {
            if !seen.insert(id) {
                break;
            }
            let Ok(dict) = doc.get_dictionary(id) else {
                break;
            };
            items.push(id);
            if depth < MAX_DEPTH
                && let Ok(child) = dict.get(b"First").and_then(Object::as_reference)
            {
                stack.push((child, depth + 1));
            }
            next = dict.get(b"Next").and_then(Object::as_reference).ok();
        }
    }
    items
}

/// Retire les destinations nommées (`/Names /Dests`, `/Dests` du catalogue) qui mènent à une
/// page hors de `live`, et rend les noms qui n'existent plus nulle part.
fn drop_dead_named_destinations(
    doc: &mut Document,
    live: &BTreeSet<ObjectId>,
) -> BTreeSet<Vec<u8>> {
    let no_names = BTreeSet::new();
    let dead = Dead {
        live,
        names: &no_names,
    };
    let mut removed = BTreeSet::new();
    let mut kept = BTreeSet::new();
    // `/Dests` du catalogue (PDF 1.1) : nom → destination.
    let dests_id = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"Dests").ok())
        .and_then(|d| d.as_reference().ok());
    let dests = match dests_id {
        Some(id) => doc.get_dictionary(id).ok(),
        None => doc
            .catalog()
            .ok()
            .and_then(|c| c.get(b"Dests").ok())
            .and_then(|d| d.as_dict().ok()),
    };
    if let Some(dests) = dests {
        for (name, value) in dests.iter() {
            if dead.dest(doc, value) {
                removed.insert(name.clone());
            } else {
                kept.insert(name.clone());
            }
        }
    }
    if !removed.is_empty() {
        let dests = match dests_id {
            Some(id) => doc.get_dictionary_mut(id).ok(),
            None => doc
                .catalog_mut()
                .ok()
                .and_then(|c| c.get_mut(b"Dests").ok())
                .and_then(|d| d.as_dict_mut().ok()),
        };
        if let Some(dests) = dests {
            for name in &removed {
                dests.remove(name);
            }
        }
    }
    // Arbre `/Names /Dests` (PDF 1.2) : chaque nœud filtre ses paires `[nom destination …]`.
    let mut stack: Vec<(ObjectId, usize)> = Vec::new();
    let mut seen = BTreeSet::new();
    let (root_id, root_kids) = match names_dests_root(doc) {
        Some(DestsRoot::Indirect(id)) => (Some(id), Vec::new()),
        Some(DestsRoot::Direct(node)) => {
            let filtered = filter_name_pairs(doc, &dead, &node, &mut removed, &mut kept);
            if let Some(pairs) = filtered
                && let Some(root) = names_dests_root_mut(doc)
            {
                root.set("Names", Object::Array(pairs));
            }
            (None, kid_refs(&node))
        }
        None => (None, Vec::new()),
    };
    stack.extend(root_id.map(|id| (id, 0)));
    stack.extend(root_kids.into_iter().map(|id| (id, 1)));
    while let Some((id, depth)) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Ok(node) = doc.get_dictionary(id).cloned() else {
            continue;
        };
        if let Some(pairs) = filter_name_pairs(doc, &dead, &node, &mut removed, &mut kept)
            && let Ok(node) = doc.get_dictionary_mut(id)
        {
            node.set("Names", Object::Array(pairs));
        }
        if depth < MAX_DEPTH {
            stack.extend(kid_refs(&node).into_iter().map(|kid| (kid, depth + 1)));
        }
    }
    // Un nom retiré d'une structure mais encore présent dans l'autre reste valable.
    removed.retain(|name| !kept.contains(name));
    removed
}

/// Où est la racine de l'arbre `/Names /Dests` : objet indirect, ou dictionnaire direct
/// (copié) rangé dans le dictionnaire `/Names`.
enum DestsRoot {
    Indirect(ObjectId),
    Direct(Dictionary),
}

fn names_dests_root(doc: &Document) -> Option<DestsRoot> {
    let names = doc.catalog().ok()?.get(b"Names").ok()?;
    let dests = doc
        .dereference(names)
        .ok()?
        .1
        .as_dict()
        .ok()?
        .get(b"Dests")
        .ok()?;
    match dests {
        Object::Reference(id) => Some(DestsRoot::Indirect(*id)),
        Object::Dictionary(dict) => Some(DestsRoot::Direct(dict.clone())),
        _ => None,
    }
}

/// La racine directe de l'arbre `/Names /Dests`, modifiable.
fn names_dests_root_mut(doc: &mut Document) -> Option<&mut Dictionary> {
    let names_id = doc.catalog().ok()?.get(b"Names").ok()?.as_reference().ok();
    let names = match names_id {
        Some(id) => doc.get_dictionary_mut(id).ok()?,
        None => doc
            .catalog_mut()
            .ok()?
            .get_mut(b"Names")
            .ok()?
            .as_dict_mut()
            .ok()?,
    };
    names.get_mut(b"Dests").ok()?.as_dict_mut().ok()
}

/// Références des `/Kids` d'un nœud d'arbre de noms.
fn kid_refs(node: &Dictionary) -> Vec<ObjectId> {
    node.get(b"Kids")
        .and_then(Object::as_array)
        .map(|kids| kids.iter().filter_map(|k| k.as_reference().ok()).collect())
        .unwrap_or_default()
}

/// Les paires `/Names` de `node` sans celles qui mènent à une page morte, ou `None` si rien
/// n'est à retirer. Note chaque nom dans `removed` ou `kept`.
fn filter_name_pairs(
    doc: &Document,
    dead: &Dead<'_>,
    node: &Dictionary,
    removed: &mut BTreeSet<Vec<u8>>,
    kept: &mut BTreeSet<Vec<u8>>,
) -> Option<Vec<Object>> {
    let items = node.get(b"Names").and_then(Object::as_array).ok()?;
    // [Apprentissage] `as_chunks::<2>()` découpe la tranche en tableaux `[T; 2]` (et rend à part
    // un éventuel élément impair) : `for [key, value] in pairs` les déstructure sans index.
    let (pairs, _odd) = items.as_chunks::<2>();
    let mut out = Vec::with_capacity(items.len());
    let mut changed = false;
    for [key, value] in pairs {
        let name = key.as_str().map(<[u8]>::to_vec).unwrap_or_default();
        if dead.dest(doc, value) {
            removed.insert(name);
            changed = true;
        } else {
            kept.insert(name);
            out.push(key.clone());
            out.push(value.clone());
        }
    }
    changed.then_some(out)
}

/// Vrai si `dict` est un marqueur de contenu `/OBJR` dont la cible `/Obj` est dans `dead`.
fn is_objr_to(dict: &Dictionary, dead: &BTreeSet<ObjectId>) -> bool {
    dict.get(b"Type").and_then(Object::as_name).ok() == Some(b"OBJR".as_slice())
        && dict
            .get(b"Obj")
            .and_then(Object::as_reference)
            .is_ok_and(|id| dead.contains(&id))
}

/// Retire de `object` (et de ce qu'il contient directement) les clés `/IRT` qui visent une
/// annotation de `dead`, et les entrées `/OBJR` qui la citent dans le `/K` d'un élément de
/// structure : le marqueur entier part (sa clé `/Obj` est obligatoire, §14.7.4.3), et avec
/// lui le texte qui décrivait l'annotation (`/Alt`, `/ActualText`, `/E`). Profondeur bornée :
/// un objet direct ne peut pas se contenir lui-même, mais un fichier hostile peut
/// l'imbriquer très loin.
fn scrub_links(
    object: &mut Object,
    dead: &BTreeSet<ObjectId>,
    dead_objr: &BTreeSet<ObjectId>,
    depth: usize,
) {
    if depth > 64 {
        return;
    }
    let is_dead_marker = |o: &Object| match o {
        Object::Dictionary(d) => is_objr_to(d, dead),
        Object::Reference(id) => dead_objr.contains(id),
        _ => false,
    };
    match object {
        Object::Dictionary(dict) => {
            if dict
                .get(b"IRT")
                .and_then(Object::as_reference)
                .is_ok_and(|id| dead.contains(&id))
            {
                dict.remove(b"IRT");
            }
            let mut removed = false;
            let mut emptied = false;
            match dict.get_mut(b"K") {
                Ok(Object::Array(kids)) => {
                    let before = kids.len();
                    kids.retain(|k| !is_dead_marker(k));
                    removed = kids.len() != before;
                    emptied = kids.is_empty();
                }
                Ok(k) if is_dead_marker(k) => {
                    removed = true;
                    emptied = true;
                }
                _ => {}
            }
            if emptied {
                dict.remove(b"K");
            }
            if removed {
                for key in [b"Alt".as_slice(), b"ActualText", b"E"] {
                    dict.remove(key);
                }
            }
            for (_, value) in dict.iter_mut() {
                scrub_links(value, dead, dead_objr, depth + 1);
            }
        }
        Object::Array(items) => {
            for item in items.iter_mut() {
                scrub_links(item, dead, dead_objr, depth + 1);
            }
        }
        Object::Stream(stream) => {
            for (_, value) in stream.dict.iter_mut() {
                scrub_links(value, dead, dead_objr, depth + 1);
            }
        }
        _ => {}
    }
}

fn subtype(dict: &Dictionary) -> Vec<u8> {
    dict.get(b"Subtype")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(|n| n.to_vec())
        .unwrap_or_default()
}

/// Copie corrigée d'un dictionnaire d'annotation, ou `None` s'il n'y a rien à changer.
///
/// \[Apprentissage\] On renvoie une **copie** plutôt que de modifier sur place : la
/// lecture de l'apparence emprunte `doc` en lecture, et Rust interdit de le modifier en
/// même temps. Calculer d'abord, écrire ensuite, lève le conflit d'emprunts.
fn cleaned(doc: &Document, dict: &Dictionary) -> Option<Dictionary> {
    let mut new_dict = dict.clone();
    let mut changed = false;
    // La clé privée de PDFium n'a rien à faire dans un fichier, quel que soit le sous-type ;
    // l'apparence générée, elle, n'est retirée que des notes (icône jaune imposée) : celle
    // des marquages est portable et attendue par les lecteurs qui ne régénèrent rien.
    if new_dict.has(b"PDFIUM_HasGeneratedAP") {
        new_dict.remove(b"PDFIUM_HasGeneratedAP");
        if subtype(dict).as_slice() == b"Text" {
            new_dict.remove(b"AP");
        }
        changed = true;
    }
    if subtype(dict).as_slice() == b"Ink"
        && !dict.has(b"InkList")
        && let Some(points) = ink_points(doc, dict)
    {
        new_dict.set("InkList", Object::Array(points));
        changed = true;
    }
    changed.then_some(new_dict)
}

/// `/InkList` déduite de l'apparence normale : un tableau de points par sous-chemin.
fn ink_points(doc: &Document, dict: &Dictionary) -> Option<Vec<Object>> {
    let ap = dict.get(b"AP").ok()?;
    let (_, ap) = doc.dereference(ap).ok()?;
    let n = ap.as_dict().ok()?.get(b"N").ok()?;
    let (_, stream) = doc.dereference(n).ok()?;
    let stream = stream.as_stream().ok()?;
    let data = crate::crypt::stream_bytes(stream)?;
    let content = Content::decode(&data).ok()?;
    let mut strokes: Vec<Vec<f32>> = Vec::new();
    let mut current: Vec<f32> = Vec::new();
    let num = |o: &Object| o.as_float().ok();
    // Une courbe de Bézier est échantillonnée en 8 segments depuis le point courant : une
    // `/InkList` réduite aux extrémités ferait d'une ellipse un losange (B25, Lot 9).
    let curve = |current: &mut Vec<f32>, c1: [f32; 2], c2: [f32; 2], end: [f32; 2]| {
        let [Some(&x0), Some(&y0)] = [current.len().checked_sub(2), current.len().checked_sub(1)]
            .map(|i| i.and_then(|i| current.get(i)))
        else {
            current.extend(end);
            return;
        };
        for step in 1..=8 {
            let t = step as f32 / 8.0;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            current.push(a * x0 + b * c1[0] + c * c2[0] + d * end[0]);
            current.push(a * y0 + b * c1[1] + c * c2[1] + d * end[1]);
        }
    };
    for op in content.operations.iter().take(100_000) {
        let v: Vec<f32> = op.operands.iter().filter_map(num).collect();
        match (op.operator.as_str(), v.as_slice()) {
            ("m", [x, y]) => {
                if current.len() >= 4 {
                    strokes.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
                current.extend([*x, *y]);
            }
            ("l", [x, y]) => current.extend([*x, *y]),
            ("c", [x1, y1, x2, y2, x, y]) => curve(&mut current, [*x1, *y1], [*x2, *y2], [*x, *y]),
            ("v", [x2, y2, x, y]) => {
                // Premier point de contrôle = point courant.
                let start = [
                    current
                        .get(current.len().wrapping_sub(2))
                        .copied()
                        .unwrap_or(*x2),
                    current.last().copied().unwrap_or(*y2),
                ];
                curve(&mut current, start, [*x2, *y2], [*x, *y]);
            }
            ("y", [x1, y1, x, y]) => curve(&mut current, [*x1, *y1], [*x, *y], [*x, *y]),
            ("re", [x, y, w, h]) => {
                if current.len() >= 4 {
                    strokes.push(std::mem::take(&mut current));
                }
                strokes.push(vec![*x, *y, x + w, *y, x + w, y + h, *x, y + h, *x, *y]);
                current.clear();
            }
            ("h", _) if current.len() >= 4 => {
                current.extend([current[0], current[1]]);
            }
            _ => {}
        }
    }
    if current.len() >= 4 {
        strokes.push(current);
    }
    if strokes.is_empty() {
        return None;
    }
    // Les points de l'apparence sont dans l'espace du formulaire : `/Matrix` les ramène dans
    // celui de la page (§8.10.1).
    let matrix: Option<Vec<f32>> = stream
        .dict
        .get(b"Matrix")
        .and_then(Object::as_array)
        .ok()
        .map(|m| m.iter().filter_map(num).collect());
    if let Some([a, b, c, d, e, f]) = matrix.as_deref() {
        for stroke in &mut strokes {
            for [x, y] in stroke.as_chunks_mut::<2>().0 {
                (*x, *y) = (a * *x + c * *y + e, b * *x + d * *y + f);
            }
        }
    }
    Some(
        strokes
            .into_iter()
            .map(|s| Object::Array(s.into_iter().map(Object::Real).collect()))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_sanity_rejects_truncated_or_page_losing_output() {
        let bytes = crate::testing::many_pages_pdf(3);
        assert!(rewrite_is_sane(3, &bytes, None));
        assert!(
            !rewrite_is_sane(4, &bytes, None),
            "nombre de pages différent"
        );
        assert!(
            !rewrite_is_sane(3, &bytes[..bytes.len() / 2], None),
            "fichier tronqué"
        );
        assert!(!rewrite_is_sane(0, b"pas un pdf", None));
        // Relecture de la PR A du Lot 13 : une réécriture sans `/Root` n'est pas saine, même
        // si la lecture réparatrice de `crypt::load_limited` saurait retrouver le catalogue.
        let mut rootless = bytes.clone();
        let k = rootless.windows(5).rposition(|w| w == b"/Root").unwrap();
        rootless[k..k + 5].copy_from_slice(b"/Roox");
        assert!(!rewrite_is_sane(3, &rootless, None));
    }
}

#[cfg(test)]
mod refuse_tests {
    use super::*;

    #[test]
    fn refuse_if_uncleaned_only_refuses_a_fallback_after_a_deletion() {
        let fallback = Saved {
            bytes: Vec::new(),
            cleaned: false,
        };
        let cleaned = Saved {
            bytes: Vec::new(),
            cleaned: true,
        };
        assert!(refuse_if_uncleaned(true, &fallback, "protection refusée").is_err());
        assert!(refuse_if_uncleaned(false, &fallback, "x").is_ok());
        assert!(refuse_if_uncleaned(true, &cleaned, "x").is_ok());
        let message = refuse_if_uncleaned(true, &fallback, "protection refusée")
            .unwrap_err()
            .to_string();
        assert!(message.contains("protection refusée"), "{message}");
        assert!(!message.contains("  "), "pas d'espaces répétés : {message}");
    }

    /// Revue du Lot 14 (I2, M13) : `into_checked` est le seul chemin des appelants de
    /// production vers les octets ; un repli non nettoyé après une suppression est refusé.
    #[test]
    fn into_checked_refuses_an_uncleaned_fallback_after_a_deletion() {
        let fallback = || Saved {
            bytes: b"brut".to_vec(),
            cleaned: false,
        };
        let err = fallback()
            .into_checked(true, "enregistrement refusé")
            .unwrap_err()
            .to_string();
        assert!(err.contains("enregistrement refusé"), "{err}");
        assert_eq!(fallback().into_checked(false, "x").unwrap(), b"brut");
        let cleaned = Saved {
            bytes: b"propre".to_vec(),
            cleaned: true,
        };
        assert_eq!(cleaned.into_checked(true, "x").unwrap(), b"propre");
    }
}

#[cfg(test)]
mod prune_tests {
    use super::*;

    /// Même résultat que `lopdf`, y compris sur des références cycliques (signets en boucle,
    /// popup et note qui se citent l'un l'autre).
    #[test]
    fn prune_matches_lopdf_on_cycles() {
        for bytes in [
            crate::testing::cyclic_outline_pdf(),
            crate::testing::note_with_popup_pdf("x"),
            crate::testing::heavy_images_pdf(),
        ] {
            let mut ours = Document::load_mem(&bytes).unwrap();
            let orphan = ours.add_object(Object::Integer(42));
            let mut theirs = ours.clone();
            assert!(prune(&mut ours) >= 1);
            theirs.prune_objects();
            let keys = |d: &Document| d.objects.keys().copied().collect::<Vec<_>>();
            assert_eq!(keys(&ours), keys(&theirs));
            assert!(!ours.objects.contains_key(&orphan));
        }
    }
}

#[cfg(test)]
mod dead_destination_tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;

    /// `rich_catalog_pdf` relu par lopdf, la page d'objet `page` retirée de l'arbre et
    /// effacée (comme PDFium le fait d'une page supprimée).
    fn rich_without(page: u32) -> Document {
        let bytes = crate::testing::rich_catalog_pdf("1.7", None);
        let mut doc = Document::load_mem(&bytes).unwrap();
        let root = doc.get_dictionary_mut((2, 0)).unwrap();
        let kids: Vec<Object> = [3, 4, 5]
            .into_iter()
            .filter(|&id| id != page)
            .map(|id| Object::Reference((id, 0)))
            .collect();
        root.set("Kids", kids);
        root.set("Count", 2);
        doc.objects.insert((page, 0), Object::Null);
        doc
    }

    fn annots(doc: &Document) -> Vec<Object> {
        page_annots(doc, (3, 0)).unwrap().1
    }

    #[test]
    fn explicit_and_pdf_1_1_named_destinations_to_a_dead_page_are_neutralized() {
        let mut doc = rich_without(5);
        neutralize_dead_destinations(&mut doc);
        // `/chap3` (dictionnaire `/Dests` du catalogue) est retiré, `(chap2)` reste.
        let dests = doc.get_dictionary((14, 0)).unwrap();
        assert!(!dests.has(b"chap3"));
        let Some(DestsRoot::Direct(tree)) = names_dests_root(&doc) else {
            panic!("arbre /Names /Dests direct attendu");
        };
        assert_eq!(tree.get(b"Names").unwrap().as_array().unwrap().len(), 2);
        // Signets « Vers chap3 » (action `GoTo` nommée) et « Direct p3 » : sans cible.
        assert!(!doc.get_dictionary((25, 0)).unwrap().has(b"A"));
        assert!(!doc.get_dictionary((26, 0)).unwrap().has(b"Dest"));
        assert!(doc.get_dictionary((24, 0)).unwrap().has(b"Dest"));
        // Liens vers la page 3 retirés ; le lien vers chap2 et le widget restent.
        assert_eq!(
            annots(&doc),
            [Object::Reference((19, 0)), Object::Reference((22, 0))]
        );
        // L'action d'ouverture vise la page 2, vivante.
        assert!(doc.catalog().unwrap().has(b"OpenAction"));
    }

    #[test]
    fn an_open_action_to_a_dead_page_is_removed() {
        let mut doc = rich_without(4);
        neutralize_dead_destinations(&mut doc);
        assert!(!doc.catalog().unwrap().has(b"OpenAction"));
        assert!(!doc.get_dictionary((24, 0)).unwrap().has(b"Dest"));
        assert_eq!(
            annots(&doc),
            [
                Object::Reference((20, 0)),
                Object::Reference((21, 0)),
                Object::Reference((22, 0))
            ]
        );
    }
}

#[cfg(test)]
mod need_appearances_tests {
    use super::*;
    use lopdf::{Stream, dictionary};

    /// Un document à une page dont les widgets (un par élément de `widgets`) portent les
    /// dictionnaires donnés, avec `/NeedAppearances true` dans l'`/AcroForm` (direct ou non).
    fn form(widgets: Vec<Dictionary>, indirect_acroform: bool) -> Document {
        let mut doc = Document::with_version("1.7");
        let refs: Vec<Object> = widgets
            .into_iter()
            .map(|mut w| {
                w.set("Type", "Annot");
                w.set("Subtype", "Widget");
                Object::Reference(doc.add_object(w))
            })
            .collect();
        let pages = doc.new_object_id();
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages,
            "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
            "Annots" => refs,
        });
        doc.objects.insert(
            pages,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1
            }),
        );
        let acro = dictionary! { "Fields" => Vec::<Object>::new(), "NeedAppearances" => true };
        let acro: Object = if indirect_acroform {
            doc.add_object(acro).into()
        } else {
            Object::Dictionary(acro)
        };
        let catalog = doc.add_object(dictionary! {
            "Type" => "Catalog", "Pages" => pages, "AcroForm" => acro
        });
        doc.trailer.set("Root", catalog);
        doc
    }

    fn with_ap() -> Dictionary {
        let stream = Stream::new(Dictionary::new(), b"q Q".to_vec());
        dictionary! { "AP" => dictionary! { "N" => Object::Stream(stream) } }
    }

    fn has_key(doc: &Document) -> bool {
        let acro = doc.catalog().unwrap().get(b"AcroForm").unwrap();
        let (_, acro) = doc.dereference(acro).unwrap();
        acro.as_dict().unwrap().has(b"NeedAppearances")
    }

    #[test]
    fn the_key_goes_when_every_widget_is_drawn() {
        for indirect in [false, true] {
            let mut doc = form(vec![with_ap(), with_ap()], indirect);
            drop_need_appearances(&mut doc);
            assert!(!has_key(&doc), "indirect : {indirect}");
        }
    }

    #[test]
    fn the_key_stays_when_a_widget_has_no_appearance() {
        let mut doc = form(vec![with_ap(), dictionary! {}], false);
        drop_need_appearances(&mut doc);
        assert!(has_key(&doc));
    }

    #[test]
    fn a_state_dictionary_must_contain_the_current_state() {
        let states = |as_state: &str| {
            let on = Stream::new(Dictionary::new(), b"q Q".to_vec());
            dictionary! {
                "AS" => Object::Name(as_state.as_bytes().to_vec()),
                "AP" => dictionary! { "N" => dictionary! { "Yes" => Object::Stream(on) } },
            }
        };
        let mut doc = form(vec![states("Yes")], false);
        drop_need_appearances(&mut doc);
        assert!(!has_key(&doc));
        let mut doc = form(vec![states("Off")], false);
        drop_need_appearances(&mut doc);
        assert!(has_key(&doc), "état /Off sans apparence");
    }
}

#[cfg(test)]
mod save_plan_tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// `many_pages_pdf(3)` dont la page 2 porte un surlignage **sans** apparence et la page 3
    /// un surlignage **avec** son `/AP /N`.
    fn markups_pdf() -> Vec<u8> {
        let mut doc = Document::load_mem(&crate::testing::many_pages_pdf(3)).unwrap();
        let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
        let highlight = |ap: Option<ObjectId>| {
            let mut d = Dictionary::from_iter(vec![
                ("Type", Object::Name(b"Annot".to_vec())),
                ("Subtype", Object::Name(b"Highlight".to_vec())),
                (
                    "Rect",
                    Object::Array(vec![10.into(), 10.into(), 50.into(), 20.into()]),
                ),
            ]);
            if let Some(id) = ap {
                d.set(
                    "AP",
                    Dictionary::from_iter(vec![("N", Object::Reference(id))]),
                );
            }
            d
        };
        let bare = doc.add_object(highlight(None));
        let stream = doc.add_object(lopdf::Stream::new(Dictionary::new(), b"q Q".to_vec()));
        let drawn = doc.add_object(highlight(Some(stream)));
        for (page, annot) in [(pages[1], bare), (pages[2], drawn)] {
            doc.get_dictionary_mut(page)
                .unwrap()
                .set("Annots", vec![Object::Reference(annot)]);
        }
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    /// Revue de la partie C : seules les pages qui en ont besoin sont rendues.
    #[test]
    fn only_pages_with_a_markup_lacking_an_appearance_are_planned() {
        let plan = save_plan(&markups_pdf(), None).unwrap();
        assert_eq!(
            plan,
            SavePlan {
                need_appearances: false,
                pages: vec![1],
            }
        );
        let plain = save_plan(&crate::testing::many_pages_pdf(3), None).unwrap();
        assert_eq!(plain.pages, Vec::<u16>::new());
    }

    #[test]
    fn widgets_are_planned_when_appearances_are_needed_or_missing() {
        // `/NeedAppearances true` : la page du widget est rendue, même s'il a une apparence.
        let plan = save_plan(&crate::testing::form_with_appearances_pdf(), None).unwrap();
        assert_eq!(
            plan,
            SavePlan {
                need_appearances: true,
                pages: vec![0],
            }
        );
        // Sans la clé, un widget qui a son apparence ne demande rien.
        let mut doc = Document::load_mem(&crate::testing::form_with_appearances_pdf()).unwrap();
        if let Ok(Object::Dictionary(form)) = doc.catalog_mut().unwrap().get_mut(b"AcroForm") {
            form.remove(b"NeedAppearances");
        }
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        assert_eq!(
            save_plan(&bytes, None).unwrap(),
            SavePlan {
                need_appearances: false,
                pages: vec![],
            }
        );
    }

    /// Un fichier que `lopdf` ne lit pas : pas de plan (l'appelant garde `/NeedAppearances`).
    #[test]
    fn unreadable_bytes_have_no_plan() {
        assert_eq!(save_plan(b"pas un pdf /Annots", None), None);
    }
}
