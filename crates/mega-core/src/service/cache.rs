// SPDX-License-Identifier: GPL-3.0-or-later
//! Cache LRU des pages rendues (PNG), borné par un budget mémoire.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use crate::geometry::Rotation;

/// Identifiant d'un document ouvert dans le service.
pub type DocId = u32;

/// Clé d'une page rendue : document, page, échelle quantifiée au centième, rotation.
///
/// L'échelle est un `f32`, qui n'implémente pas `Eq`/`Hash` (à cause de `NaN`) ; on la
/// stocke multipliée par 100 et arrondie : 1,5 et 1,504 partagent la même image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PageKey {
    pub doc: DocId,
    pub page: u16,
    pub scale_x100: u32,
    pub rotation: Rotation,
}

impl PageKey {
    pub(crate) fn new(doc: DocId, page: u16, scale: f32, rotation: Rotation) -> PageKey {
        PageKey {
            doc,
            page,
            scale_x100: (scale * 100.0).round().max(1.0) as u32,
            rotation,
        }
    }
}

/// Cache « le moins récemment utilisé sort en premier », borné en octets.
///
/// \[Apprentissage\] Deux structures coopèrent : la `HashMap` pour retrouver une image (et
/// son « tic » d'utilisation) en O(1), et un `BTreeMap` tic → clé, trié, dont le premier
/// élément est le moins récemment utilisé : marquer une page utilisée coûte O(log n), là où
/// chercher sa place dans une liste coûtait O(n) (A19, Lot 9). Les images sont partagées en
/// `Arc<Vec<u8>>` : la réponse envoyée au frontend et l'entrée du cache pointent vers les
/// **mêmes** octets, sans copie.
pub(crate) struct PngCache {
    budget: usize,
    used: usize,
    entries: HashMap<PageKey, (Arc<Vec<u8>>, u64)>,
    order: BTreeMap<u64, PageKey>,
    tick: u64,
}

impl PngCache {
    pub(crate) fn new(budget_bytes: usize) -> PngCache {
        PngCache {
            budget: budget_bytes,
            used: 0,
            entries: HashMap::new(),
            order: BTreeMap::new(),
            tick: 0,
        }
    }

    /// Retourne l'image si présente, et la marque comme récemment utilisée.
    pub(crate) fn get(&mut self, key: &PageKey) -> Option<Arc<Vec<u8>>> {
        let png = Arc::clone(&self.entries.get(key)?.0);
        self.touch(key);
        Some(png)
    }

    /// Insère (ou remplace) une image, puis évince les plus anciennes au-delà du budget.
    pub(crate) fn insert(&mut self, key: PageKey, png: Arc<Vec<u8>>) {
        self.used += png.len();
        if let Some((old, old_tick)) = self.entries.insert(key, (png, 0)) {
            self.used -= old.len();
            self.order.remove(&old_tick);
        }
        self.touch(&key);
        while self.used > self.budget && self.order.len() > 1 {
            let Some((_, oldest)) = self.order.pop_first() else {
                break;
            };
            if let Some((evicted, _)) = self.entries.remove(&oldest) {
                self.used -= evicted.len();
            }
        }
    }

    /// Oublie toutes les pages d'un document (fermeture, changement de génération).
    pub(crate) fn invalidate_doc(&mut self, doc: DocId) {
        let keys: Vec<PageKey> = self
            .entries
            .keys()
            .filter(|k| k.doc == doc)
            .copied()
            .collect();
        for k in keys {
            if let Some((png, tick)) = self.entries.remove(&k) {
                self.used -= png.len();
                self.order.remove(&tick);
            }
        }
    }

    /// Marque `key` comme la plus récemment utilisée.
    fn touch(&mut self, key: &PageKey) {
        let Some((_, tick)) = self.entries.get_mut(key) else {
            return;
        };
        self.order.remove(tick);
        self.tick += 1;
        *tick = self.tick;
        self.order.insert(self.tick, *key);
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(n: usize) -> Arc<Vec<u8>> {
        Arc::new(vec![0; n])
    }

    #[test]
    fn evicts_least_recently_used_beyond_budget() {
        let mut c = PngCache::new(250);
        let k = |p: u16| PageKey::new(1, p, 1.0, Rotation::None);
        c.insert(k(0), png(100));
        c.insert(k(1), png(100));
        assert!(c.get(&k(0)).is_some()); // 0 devient le plus récent
        c.insert(k(2), png(100)); // dépasse : évince le moins récent = 1
        assert!(c.get(&k(1)).is_none());
        assert!(c.get(&k(0)).is_some() && c.get(&k(2)).is_some());
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn invalidate_doc_removes_only_that_document() {
        let mut c = PngCache::new(1_000);
        c.insert(PageKey::new(1, 0, 1.0, Rotation::None), png(10));
        c.insert(PageKey::new(2, 0, 1.0, Rotation::None), png(10));
        c.invalidate_doc(1);
        assert!(c.get(&PageKey::new(1, 0, 1.0, Rotation::None)).is_none());
        assert!(c.get(&PageKey::new(2, 0, 1.0, Rotation::None)).is_some());
    }

    #[test]
    fn scale_is_quantised() {
        assert_eq!(
            PageKey::new(1, 0, 1.504, Rotation::None),
            PageKey::new(1, 0, 1.5, Rotation::None)
        );
    }

    /// Beaucoup de clés : l'ordre d'éviction reste celui des derniers accès (A19, Lot 9).
    #[test]
    fn touch_keeps_lru_order() {
        let k = |p: u16| PageKey::new(1, p, 1.0, Rotation::None);
        let mut c = PngCache::new(10_000);
        for p in 0..10_000u16 {
            c.insert(k(p), png(1));
        }
        assert!(c.get(&k(0)).is_some()); // 0 redevient le plus récent
        c.insert(k(10_000), png(1)); // dépasse : évince le moins récent, 1
        assert!(c.get(&k(1)).is_none());
        assert!(c.get(&k(0)).is_some() && c.get(&k(2)).is_some());
        assert_eq!(c.len(), 10_000);
    }
}
