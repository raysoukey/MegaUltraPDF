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
    /// PDFium n'expose pas de déplacement : on copie les pages depuis une copie du document
    /// chargée des mêmes octets, puis on supprime les originales. Les signets qui visaient
    /// une page déplacée peuvent être perdus (limite connue, voir la spec du Lot 2).
    pub fn move_pages(&mut self, pages: &[u16], to: u16) -> CoreResult<()> {
        self.mark_changed();
        let guard = engine::guard();
        let moved = self.unique_indices_unlocked(pages)?;
        let remaining = self.count_unlocked() as usize - moved.len();
        if to as usize > remaining {
            return Err(CoreError::PageOutOfRange {
                index: to,
                count: remaining as u16,
            });
        }
        let snapshot = engine::load_bytes(&guard, self.inner().save_to_bytes()?, self.password())?;
        for i in moved.iter().rev() {
            self.inner().pages().get(*i)?.delete()?;
        }
        copy_pages(self.inner_mut(), &snapshot, &moved, PdfPageIndex::from(to))
    }

    /// Nouveau document composé des pages `pages` (dans l'ordre donné) ; l'original est intact.
    pub fn extract_pages(&self, pages: &[u16]) -> CoreResult<Vec<u8>> {
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

/// Fusionne des documents dans l'ordre donné en un nouveau PDF (octets).
pub fn merge(docs: &[&Document]) -> CoreResult<Vec<u8>> {
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

/// Scinde `doc` en autant de PDF que de groupes dans `parts` (indices 0-based).
pub fn split(doc: &Document, parts: &[Vec<u16>]) -> CoreResult<Vec<Vec<u8>>> {
    if parts.is_empty() {
        return Err(CoreError::InvalidOperation(
            "aucune partie à produire".to_string(),
        ));
    }
    parts.iter().map(|p| doc.extract_pages(p)).collect()
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
