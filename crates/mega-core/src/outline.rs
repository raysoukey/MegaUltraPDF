// SPDX-License-Identifier: GPL-3.0-or-later
//! Signets (table des matières) d'un document.

use pdfium_render::prelude::PdfBookmark;
use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::engine;
use crate::error::CoreResult;

/// Nombre maximal de signets lus. Au-delà, la lecture s'arrête silencieusement.
///
/// Un PDF malformé ou hostile peut contenir une chaîne `/Next` qui boucle sur elle-même ;
/// PDFium ne le détecte pas. Sans borne, l'acteur PDFium tournerait sans fin et toute
/// l'application serait figée. Le fichier est une donnée non fiable.
pub const MAX_OUTLINE_NODES: usize = 10_000;

/// Profondeur maximale de l'arbre de signets (protège la pile de la récursion).
pub const MAX_OUTLINE_DEPTH: usize = 64;

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

/// Budget partagé par toute la lecture d'un arbre : nombre de nœuds restants.
struct Budget {
    remaining: usize,
}

impl Budget {
    /// Consomme un nœud ; `false` quand le budget est épuisé.
    fn take(&mut self) -> bool {
        if self.remaining == 0 {
            return false;
        }
        self.remaining -= 1;
        true
    }
}

impl Document {
    /// L'arbre des signets ; vide si le document n'en a pas. La lecture est bornée
    /// ([`MAX_OUTLINE_NODES`], [`MAX_OUTLINE_DEPTH`]) : un arbre cyclique ou démesuré est
    /// tronqué, jamais parcouru indéfiniment.
    pub fn outline(&self) -> CoreResult<Vec<OutlineNode>> {
        let _guard = engine::guard();
        let mut budget = Budget {
            remaining: MAX_OUTLINE_NODES,
        };
        Ok(siblings(self.inner().bookmarks().root(), 0, &mut budget))
    }
}

/// Suit la chaîne `next_sibling` à partir de `first` (une liste chaînée, comme `/Next`
/// dans le fichier), en convertissant chaque signet et ses enfants.
fn siblings(first: Option<PdfBookmark<'_>>, depth: usize, budget: &mut Budget) -> Vec<OutlineNode> {
    let mut nodes = Vec::new();
    let mut current = first;
    while let Some(b) = current {
        if !budget.take() {
            break;
        }
        nodes.push(node(&b, depth, budget));
        current = b.next_sibling();
    }
    nodes
}

/// Convertit un signet ; ses enfants ne sont lus que sous la profondeur maximale.
fn node(b: &PdfBookmark<'_>, depth: usize, budget: &mut Budget) -> OutlineNode {
    let children = if depth + 1 < MAX_OUTLINE_DEPTH {
        siblings(b.first_child(), depth + 1, budget)
    } else {
        Vec::new()
    };
    OutlineNode {
        title: b.title().unwrap_or_default(),
        page: target_page(b),
        children,
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
