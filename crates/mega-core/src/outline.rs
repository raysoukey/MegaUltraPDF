// SPDX-License-Identifier: GPL-3.0-or-later
//! Signets (table des matières) d'un document.

use pdfium_render::prelude::PdfBookmark;
use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::engine;
use crate::error::CoreResult;

/// Un signet et ses enfants.
///
/// \[Apprentissage\] Une structure **récursive** : `children` contient des `OutlineNode`.
/// C'est possible parce que `Vec` range ses éléments sur le tas ; `children: OutlineNode`
/// (sans `Vec` ni `Box`) serait refusé, la taille du type étant alors infinie.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutlineNode {
    /// Libellé du signet.
    pub title: String,
    /// Page cible (à partir de 0), si le signet en a une.
    pub page: Option<u16>,
    /// Sous-signets, dans l'ordre.
    pub children: Vec<OutlineNode>,
}

impl Document {
    /// L'arbre des signets ; vide si le document n'en a pas.
    pub fn outline(&self) -> CoreResult<Vec<OutlineNode>> {
        let _guard = engine::guard();
        // `root()` est le premier signet de premier niveau ; on suit la chaîne `next_sibling`
        // à la main : c'est une liste chaînée, exactement comme dans le fichier PDF (`/Next`).
        let mut nodes = Vec::new();
        let mut current = self.inner().bookmarks().root();
        while let Some(b) = current {
            nodes.push(node(&b));
            current = b.next_sibling();
        }
        Ok(nodes)
    }
}

/// Convertit récursivement un signet PDFium en `OutlineNode`.
fn node(b: &PdfBookmark<'_>) -> OutlineNode {
    OutlineNode {
        title: b.title().unwrap_or_default(),
        page: target_page(b),
        children: b.iter_direct_children().map(|c| node(&c)).collect(),
    }
}

/// Page visée par un signet : destination directe, sinon action « aller à ».
fn target_page(b: &PdfBookmark<'_>) -> Option<u16> {
    let from_dest = b.destination().and_then(|d| d.page_index().ok());
    // L'index (un entier, `Copy`) est extrait avant que l'action temporaire ne disparaisse :
    // renvoyer la destination elle-même emprunterait `a`, détruite à la fin de la fermeture.
    let from_action = || {
        let a = b.action()?;
        let local = a.as_local_destination_action()?;
        let index = local.destination().ok()?.page_index().ok()?;
        Some(index)
    };
    from_dest
        .or_else(from_action)
        .and_then(|i| u16::try_from(i).ok())
}
