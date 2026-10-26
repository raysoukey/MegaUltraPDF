// SPDX-License-Identifier: GPL-3.0-or-later
//! File de requêtes priorisée de l'acteur.

use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

use serde::{Deserialize, Serialize};

/// Priorité d'un rendu : ce que l'utilisateur regarde passe avant ce qu'il pourrait regarder.
///
/// L'ordre de déclaration compte : `derive(PartialOrd, Ord)` compare les variantes dans cet
/// ordre, donc `Visible > Prefetch > Thumbnail`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    /// Vignette de la barre latérale.
    Thumbnail,
    /// Page voisine de celles affichées (préchargement).
    Prefetch,
    /// Page actuellement à l'écran.
    Visible,
}

/// Une entrée de la file : la priorité, puis l'ancienneté (plus ancien d'abord).
///
/// \[Apprentissage\] `BinaryHeap` sort toujours le **plus grand** élément. On veut la plus
/// haute priorité, et à priorité égale la requête la plus ancienne (plus petit numéro de
/// séquence) : `Reverse(seq)` inverse la comparaison du numéro pour que le plus petit soit
/// « le plus grand ». `Ord` est dérivé champ par champ, dans l'ordre : priorité d'abord.
#[derive(Debug, PartialEq, Eq)]
struct Entry<T> {
    priority: Priority,
    seq: Reverse<u64>,
    item: T,
}

impl<T: Eq> Ord for Entry<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority
            .cmp(&other.priority)
            .then_with(|| self.seq.cmp(&other.seq))
    }
}

impl<T: Eq> PartialOrd for Entry<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// File qui rend d'abord la priorité la plus haute, puis la plus ancienne.
pub(crate) struct PriorityQueue<T: Eq> {
    heap: BinaryHeap<Entry<T>>,
    next_seq: u64,
}

impl<T: Eq> Default for PriorityQueue<T> {
    fn default() -> Self {
        PriorityQueue {
            heap: BinaryHeap::new(),
            next_seq: 0,
        }
    }
}

impl<T: Eq> PriorityQueue<T> {
    pub(crate) fn push(&mut self, priority: Priority, item: T) {
        self.heap.push(Entry {
            priority,
            seq: Reverse(self.next_seq),
            item,
        });
        self.next_seq += 1;
    }

    pub(crate) fn pop(&mut self) -> Option<T> {
        self.heap.pop().map(|e| e.item)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highest_priority_then_oldest() {
        let mut q = PriorityQueue::default();
        q.push(Priority::Thumbnail, "t1");
        q.push(Priority::Visible, "v1");
        q.push(Priority::Prefetch, "p1");
        q.push(Priority::Visible, "v2");
        q.push(Priority::Thumbnail, "t2");
        let order: Vec<_> = std::iter::from_fn(|| q.pop()).collect();
        assert_eq!(order, vec!["v1", "v2", "p1", "t1", "t2"]);
        assert!(q.is_empty());
    }

    #[test]
    fn priority_order_is_visible_first() {
        assert!(Priority::Visible > Priority::Prefetch);
        assert!(Priority::Prefetch > Priority::Thumbnail);
    }
}
