// SPDX-License-Identifier: GPL-3.0-or-later
//! Opérations sur la structure d'un document : rotation enregistrée, suppression,
//! insertion, déplacement, extraction, fusion, scission.
//!
//! Toutes ces opérations modifient le document **en mémoire** ; rien n'est écrit sur
//! disque avant `Document::save` / `save_as` (module `edit`). Chaque méthode prend le
//! jeton PDFium une seule fois et valide tous ses arguments **avant** de modifier quoi que
//! ce soit : une erreur laisse le document tel qu'il était.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use pdfium_render::prelude::{PdfDocument, PdfPageIndex, PdfPagePaperSize, PdfPoints};

use crate::document::{Document, PageSize};
use crate::engine::{self, PdfiumGuard};
use crate::error::{CoreError, CoreResult};
use crate::geometry::Rotation;
use crate::pagespec::PageSpec;

mod carry;
mod reorder;

impl Document {
    /// Rotation **enregistrée** dans le fichier pour la page `index` (distincte de la
    /// rotation d'affichage choisie par l'utilisateur).
    pub fn page_rotation(&self, index: u16) -> CoreResult<Rotation> {
        let _guard = engine::guard();
        let page = self
            .inner()
            .pages()
            .get(self.checked_index_unlocked(index)?)?;
        Ok(Rotation::from_pdfium(page.rotation()?))
    }

    /// Ajoute `quarter_turns` quarts de tour (positifs = horaire, négatifs = antihoraire)
    /// à la rotation enregistrée des pages `pages`.
    pub fn rotate_pages(&mut self, pages: &[u16], quarter_turns: i32) -> CoreResult<()> {
        self.mark_changed();
        let _guard = engine::guard();
        let indices = self.unique_indices_unlocked(pages)?;
        for i in indices {
            let mut page = self.inner().pages().get(i)?;
            let current = Rotation::from_pdfium(page.rotation()?).degrees() as i32;
            let next = (current + quarter_turns * 90).rem_euclid(360) as u16;
            page.set_rotation(Rotation::from_degrees(next).to_pdfium());
        }
        Ok(())
    }

    /// Supprime les pages `pages`. Refuse de supprimer la dernière page du document.
    pub fn delete_pages(&mut self, pages: &[u16]) -> CoreResult<()> {
        self.ensure_cleanable()?;
        self.mark_changed();
        let _guard = engine::guard();
        let indices = self.unique_indices_unlocked(pages)?;
        if indices.len() >= self.count_unlocked() as usize {
            return Err(CoreError::InvalidOperation(
                "un document doit garder au moins une page".to_string(),
            ));
        }
        // Du plus grand index au plus petit : chaque suppression décale les suivantes.
        for i in indices.into_iter().rev() {
            self.inner().pages().get(i)?.delete()?;
        }
        Ok(())
    }

    /// Insère une page vide de dimensions `size` à l'index `at` (`at == page_count` = à la fin).
    pub fn insert_blank_page(&mut self, at: u16, size: PageSize) -> CoreResult<()> {
        self.mark_changed();
        let _guard = engine::guard();
        ensure_room(self.count_unlocked(), 1)?;
        let at = self.insertion_index_unlocked(at)?;
        let paper = PdfPagePaperSize::Custom(
            PdfPoints::new(size.width_pt),
            PdfPoints::new(size.height_pt),
        );
        self.inner_mut()
            .pages_mut()
            .create_page_at_index(paper, at)?;
        Ok(())
    }

    /// Insère, à l'index `at`, les pages `pages` d'un autre document, dans l'ordre donné.
    pub fn insert_pages_from(
        &mut self,
        other: &Document,
        pages: &[u16],
        at: u16,
    ) -> CoreResult<()> {
        self.mark_changed();
        let _guard = engine::guard();
        ensure_room(self.count_unlocked(), pages.len())?;
        let at = self.insertion_index_unlocked(at)?;
        // `other` est validé sans reprendre le jeton (le `Mutex` n'est pas réentrant).
        let source: Vec<PdfPageIndex> = pages
            .iter()
            .map(|&p| other.checked_index_unlocked(p))
            .collect::<CoreResult<_>>()?;
        copy_pages(self.inner_mut(), other.inner(), &source, at)
    }

    /// Déplace les pages `pages` (dans l'ordre du document) pour qu'elles commencent à
    /// l'index `to`, **compté une fois ces pages retirées** (`0..=page_count - pages.len()`).
    ///
    /// PDFium n'expose pas de déplacement, et une copie de page serait un nouvel objet que plus
    /// rien ne viserait (signets, liens, balisage, formulaire : audit E4). Depuis le Lot 14, on
    /// réordonne donc l'arbre des pages avec `lopdf` (`reorder::reorder_kids`) : la page reste
    /// le même objet. Le document est déchiffré puis rechiffré à l'identique
    /// (`crypt::LopdfSession`) et rechargé dans PDFium.
    ///
    /// Tout est vérifié et calculé **avant** de marquer le document modifié : un déplacement
    /// refusé laisse `changed` faux, sinon l'enregistrement suivant réécrirait le fichier (et
    /// romprait une signature).
    pub fn move_pages(&mut self, pages: &[u16], to: u16) -> CoreResult<()> {
        let bytes = self.reordered_bytes(pages, to)?;
        self.mark_changed();
        self.reload_from_bytes(bytes)
    }

    /// Octets du document une fois les pages déplacées (voir [`Document::move_pages`]), sans
    /// toucher au document.
    fn reordered_bytes(&self, pages: &[u16], to: u16) -> CoreResult<Vec<u8>> {
        let count = {
            let _guard = engine::guard();
            let moved = self.unique_indices_unlocked(pages)?;
            let count = self.count_unlocked();
            let remaining = usize::from(count) - moved.len();
            if usize::from(to) > remaining {
                return Err(CoreError::PageOutOfRange {
                    index: to,
                    count: remaining as u16,
                });
            }
            count
        };
        // `to_bytes` et `reload_from_bytes` prennent eux-mêmes le jeton (non réentrant).
        let mut session = crate::crypt::LopdfSession::load(&self.to_bytes()?, self.password())?;
        // Index de PDFium et de lopdf doivent désigner les mêmes pages.
        if session.doc.get_pages().len() != usize::from(count) {
            return Err(CoreError::InvalidOperation(
                "déplacement impossible : lopdf ne voit pas les mêmes pages que PDFium".to_string(),
            ));
        }
        reorder::reorder_kids(&mut session.doc, pages, to)?;
        session.finish()
    }

    /// Nouveau document composé des pages `pages` (dans l'ordre donné) ; l'original est intact.
    /// Formulaire, signets, pièces jointes, étiquettes, calques et métadonnées du catalogue qui
    /// concernent ces pages sont reportés (`carry`, audit E3). Le résultat passe par le
    /// nettoyage d'enregistrement (`cleanup::before_save`) : une note caviardée encore citée
    /// par une réponse ne doit pas revenir dans l'extrait (relecture du Lot 9). Sert aussi à la
    /// scission.
    pub fn extract_pages(&self, pages: &[u16]) -> CoreResult<Vec<u8>> {
        let source = carry_source(self)?;
        self.extract_with(pages, source.as_ref())
    }

    /// [`Document::extract_pages`] avec la source déjà relue par `lopdf` (`carry_source`) : la
    /// scission la relit une fois pour toutes ses parties.
    fn extract_with(&self, pages: &[u16], source: Option<&lopdf::Document>) -> CoreResult<Vec<u8>> {
        let copied = self.copy_pages_raw(pages)?;
        let carried = with_catalogs(copied, &[(source, pages)])?;
        // Document neuf assemblé par PDFium : jamais chiffré.
        crate::cleanup::before_save(&carried, None, self.file_version())?
            .into_checked(self.needs_cleanup, "extraction refusée")
    }

    /// Copie **brute** par PDFium des pages `pages` (dans l'ordre donné) dans un document neuf,
    /// sérialisée : ni report du catalogue, ni nettoyage. Étape commune à l'extraction et aux
    /// tests de la passe de report (`carry`), qui ont besoin de ce que PDFium seul produit.
    pub(crate) fn copy_pages_raw(&self, pages: &[u16]) -> CoreResult<Vec<u8>> {
        let guard = engine::guard();
        let source: Vec<PdfPageIndex> = pages
            .iter()
            .map(|&p| self.checked_index_unlocked(p))
            .collect::<CoreResult<_>>()?;
        if source.is_empty() {
            return Err(CoreError::InvalidOperation(
                "aucune page à extraire".to_string(),
            ));
        }
        let mut target = new_document(&guard)?;
        copy_pages(&mut target, self.inner(), &source, 0)?;
        Ok(target.save_to_bytes()?)
    }

    /// Indices valides, triés, sans doublon ; erreur si la liste est vide ou hors bornes.
    fn unique_indices_unlocked(&self, pages: &[u16]) -> CoreResult<Vec<PdfPageIndex>> {
        if pages.is_empty() {
            return Err(CoreError::InvalidOperation(
                "aucune page sélectionnée".to_string(),
            ));
        }
        let set: BTreeSet<u16> = pages.iter().copied().collect();
        set.into_iter()
            .map(|p| self.checked_index_unlocked(p))
            .collect()
    }

    /// Index d'insertion valide : de 0 à `page_count` inclus.
    fn insertion_index_unlocked(&self, at: u16) -> CoreResult<PdfPageIndex> {
        let count = self.count_unlocked();
        if at > count {
            return Err(CoreError::PageOutOfRange { index: at, count });
        }
        Ok(PdfPageIndex::from(at))
    }
}

/// Refuse d'ajouter `added` pages à un document qui en a `count` si le total dépasse
/// 65 535 : l'index de page est un `u16` partout (A4, Lot 9 ; l'ouverture refuse déjà un
/// fichier plus long).
pub(crate) fn ensure_room(count: u16, added: usize) -> CoreResult<()> {
    if usize::from(count).saturating_add(added) > usize::from(u16::MAX) {
        return Err(CoreError::InvalidOperation(
            "un document ne peut pas dépasser 65 535 pages".to_string(),
        ));
    }
    Ok(())
}

/// Copie `source_pages` de `source` dans `target` à partir de `at`, dans l'ordre donné.
fn copy_pages(
    target: &mut PdfDocument<'static>,
    source: &PdfDocument<'static>,
    source_pages: &[PdfPageIndex],
    at: PdfPageIndex,
) -> CoreResult<()> {
    for (k, &p) in source_pages.iter().enumerate() {
        target
            .pages_mut()
            .copy_page_from_document(source, p, at + k as PdfPageIndex)?;
    }
    Ok(())
}

/// Un document vide, prêt à recevoir des pages (jeton déjà pris par l'appelant).
fn new_document(_guard: &PdfiumGuard) -> CoreResult<PdfDocument<'static>> {
    Ok(engine::pdfium()?.create_new_pdf()?)
}

/// Fusionne des documents dans l'ordre donné en un nouveau PDF (octets). Le catalogue de
/// chaque source est reporté (`carry`, audit E3) : formulaires (un nom de champ déjà pris
/// reçoit ` (2)`, ` (3)`…), signets à la suite, pièces jointes, étiquettes, calques ; la langue,
/// les préférences d'affichage et les métadonnées XMP sont celles de la première source qui
/// en a.
pub fn merge(docs: &[&Document]) -> CoreResult<Vec<u8>> {
    let copied = merge_raw(docs)?;
    let pages: Vec<Vec<u16>> = docs.iter().map(|d| (0..d.page_count()).collect()).collect();
    let read: Vec<Option<lopdf::Document>> = docs
        .iter()
        .map(|doc| carry_source(doc))
        .collect::<CoreResult<_>>()?;
    let sources: Vec<(Option<&lopdf::Document>, &[u16])> = read
        .iter()
        .zip(&pages)
        .map(|(source, pages)| (source.as_ref(), pages.as_slice()))
        .collect();
    let carried = with_catalogs(copied, &sources)?;
    // Le plus récent des en-têtes sources : la fusion ne doit pas rabaisser la version.
    let version = docs.iter().filter_map(|d| d.file_version()).max();
    let deleted = docs.iter().any(|d| d.needs_cleanup);
    crate::cleanup::before_save(&carried, None, version)?.into_checked(deleted, "fusion refusée")
}

/// Fusion **brute** par PDFium (document neuf, pages de chaque source à la suite), sérialisée :
/// ni report du catalogue, ni nettoyage. Étape commune à [`merge`] et aux tests de `carry`.
pub(crate) fn merge_raw(docs: &[&Document]) -> CoreResult<Vec<u8>> {
    if docs.is_empty() {
        return Err(CoreError::InvalidOperation(
            "aucun document à fusionner".to_string(),
        ));
    }
    let guard = engine::guard();
    let mut target = new_document(&guard)?;
    for doc in docs {
        target.pages_mut().append(doc.inner())?;
    }
    Ok(target.save_to_bytes()?)
}

/// Reporte sur `copied` (document neuf de PDFium) le catalogue de chaque source `(source relue
/// par lopdf, pages copiées)` ; les pages d'une source suivent celles des sources précédentes.
/// La `k`-ième source (k ≥ 2) renomme ses champs déjà pris avec le suffixe ` (k)`. Une source
/// que `lopdf` n'a pas relue (`None`) garde la seule copie de PDFium (comportement d'avant le
/// Lot 14).
fn with_catalogs(
    copied: Vec<u8>,
    sources: &[(Option<&lopdf::Document>, &[u16])],
) -> CoreResult<Vec<u8>> {
    let Ok(mut target) = crate::crypt::load_limited(&copied, None) else {
        return Ok(copied);
    };
    let mut first = 0;
    for (k, (source, pages)) in sources.iter().enumerate() {
        if let Some(source) = source {
            let suffix = (k > 0).then(|| format!(" ({})", k + 1));
            carry::carry_catalog(source, &mut target, pages, first, suffix.as_deref());
        }
        first += pages.len();
    }
    let mut out = Vec::new();
    target.save_to(&mut out)?;
    Ok(out)
}

/// `doc` relu par `lopdf`, objets en clair (déchiffré avec son mot de passe), pour la passe de
/// report (lecture seule : une même relecture sert à toutes les parties d'une scission) ; `None` si `lopdf` ne le relit pas, ou n'y voit pas le même nombre de pages que
/// PDFium (les index de pages ne se correspondraient plus).
fn carry_source(doc: &Document) -> CoreResult<Option<lopdf::Document>> {
    // Une modification non enregistrée n'est que dans PDFium : on relit alors son état actuel.
    // [Apprentissage] `owned` n'est initialisée que dans la branche qui en a besoin ; Rust
    // vérifie qu'on ne lit jamais une variable avant de lui avoir donné une valeur.
    let owned;
    let bytes: &[u8] = if doc.changed {
        owned = doc.to_bytes()?;
        &owned
    } else {
        doc.file_bytes()
    };
    let Ok(session) = crate::crypt::LopdfSession::load(bytes, doc.password()) else {
        return Ok(None);
    };
    let passthrough = session.keeps_object_numbers();
    let mut source = session.doc;
    if source.get_pages().len() != usize::from(doc.page_count()) {
        return Ok(None);
    }
    // Fichier `Identity` : ses pièces jointes restent chiffrées (`/EFF`) ; copiées dans le
    // document neuf, non chiffré, elles seraient illisibles.
    if passthrough && let Some(names) = carry::names_dict_mut(&mut source) {
        names.remove(b"EmbeddedFiles");
    }
    Ok(Some(source))
}

/// Scinde `doc` en autant de PDF que de groupes dans `parts` (indices 0-based).
pub fn split(doc: &Document, parts: &[Vec<u16>]) -> CoreResult<Vec<Vec<u8>>> {
    if parts.is_empty() {
        return Err(CoreError::InvalidOperation(
            "aucune partie à produire".to_string(),
        ));
    }
    let source = carry_source(doc)?;
    parts
        .iter()
        .map(|p| doc.extract_with(p, source.as_ref()))
        .collect()
}

/// Façon de scinder un document (miroir TypeScript : `SplitMode`, et options du CLI).
///
/// La règle vit ici, dans le cœur : le CLI et l'application appellent la même fonction
/// [`plan_split`] au lieu de réanalyser « 1,2-3 » chacun de leur côté.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SplitMode {
    /// Plages explicites (« 1-2,3-5 »), une partie par plage.
    Ranges {
        /// La spécification saisie par l'utilisateur.
        spec: String,
    },
    /// Une partie toutes les `n` pages (`n ≥ 1`).
    Every {
        /// Taille d'une partie.
        n: u16,
    },
    /// Une partie par signet de premier niveau.
    Bookmarks,
}

/// Les parties (index 0-based) qu'une scission produirait, sans rien écrire.
pub fn plan_split(doc: &Document, mode: &SplitMode) -> CoreResult<Vec<Vec<u16>>> {
    let count = doc.page_count();
    match mode {
        SplitMode::Ranges { spec } => PageSpec::ranges(spec, count),
        SplitMode::Every { n } => {
            if *n == 0 {
                return Err(CoreError::InvalidOperation(
                    "la taille d'une partie doit être d'au moins une page".to_string(),
                ));
            }
            Ok(PageSpec::every(*n, count))
        }
        SplitMode::Bookmarks => {
            let starts: Vec<u16> = doc.outline()?.iter().filter_map(|n| n.page).collect();
            if starts.is_empty() {
                return Err(CoreError::InvalidOperation(
                    "ce document n'a pas de signets".to_string(),
                ));
            }
            Ok(PageSpec::split_at(&starts, count))
        }
    }
}

/// Écrit les parties dans `out_dir` sous les noms `<base_name>-N.pdf` et retourne les chemins.
///
/// Rien n'est écrit si un fichier de sortie existe déjà (sauf `overwrite`) ou si `base_name`
/// n'est pas un simple nom de fichier : l'utilisateur ne perd jamais un fichier sans l'avoir
/// demandé. Chaque fichier est écrit de façon atomique.
pub fn split_to_dir(
    doc: &Document,
    parts: &[Vec<u16>],
    out_dir: &Path,
    base_name: &str,
    overwrite: bool,
) -> CoreResult<Vec<PathBuf>> {
    let base = base_name.trim();
    if base.is_empty() || base.contains(['/', '\\', '\0']) || base.contains("..") {
        return Err(CoreError::InvalidOperation(format!(
            "préfixe de fichier invalide : « {base_name} »"
        )));
    }
    if parts.is_empty() {
        return Err(CoreError::InvalidOperation(
            "aucune partie à produire".to_string(),
        ));
    }
    let paths: Vec<PathBuf> = (1..=parts.len())
        .map(|i| out_dir.join(format!("{base}-{i}.pdf")))
        .collect();
    if let Some(existing) = paths.iter().find(|p| !overwrite && p.exists()) {
        return Err(CoreError::InvalidOperation(format!(
            "« {} » existe déjà",
            existing.display()
        )));
    }
    let files = split(doc, parts)?;
    std::fs::create_dir_all(out_dir)?;
    for (path, bytes) in paths.iter().zip(&files) {
        crate::edit::write_atomically(path, bytes)?;
    }
    Ok(paths)
}

#[cfg(test)]
mod room_tests {
    use super::*;

    /// Un document ne dépasse jamais 65 535 pages : l'index de page est un `u16` (A4).
    #[test]
    fn inserting_beyond_65535_pages_is_refused() {
        assert!(ensure_room(65_534, 1).is_ok());
        assert!(matches!(
            ensure_room(65_535, 1),
            Err(CoreError::InvalidOperation(_))
        ));
        assert!(ensure_room(10, 65_530).is_err());
    }
}
