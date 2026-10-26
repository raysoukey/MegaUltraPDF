// SPDX-License-Identifier: GPL-3.0-or-later
//! Registre des documents ouverts, identifiés par un entier.
//!
//! Le frontend ne manipule jamais un `Document` directement : il reçoit un
//! identifiant à l'ouverture et le renvoie à chaque commande. Plusieurs documents
//! peuvent être ouverts en même temps (onglets, à partir du Lot 1).

use std::collections::HashMap;

use mega_core::Document;

/// Identifiant d'un document ouvert, unique pendant la vie du processus.
pub type DocId = u32;

/// Les documents ouverts. Stocké dans l'état Tauri derrière un `Mutex`.
///
/// \[Apprentissage\] Le registre **possède** les documents (`HashMap<DocId, Document>`).
/// `insert` prend un `Document` par valeur (transfert de propriété), `get` prête une
/// référence, `remove` détruit le document : PDFium libère alors ses ressources.
#[derive(Default)]
pub struct DocumentRegistry {
    next_id: DocId,
    docs: HashMap<DocId, Document>,
}

impl DocumentRegistry {
    /// Enregistre un document et retourne son identifiant (jamais réutilisé).
    pub fn insert(&mut self, doc: Document) -> DocId {
        self.next_id += 1;
        let id = self.next_id;
        self.docs.insert(id, doc);
        id
    }

    /// Le document `id`, s'il est encore ouvert.
    pub fn get(&self, id: DocId) -> Option<&Document> {
        self.docs.get(&id)
    }

    /// Ferme le document `id`. Retourne `false` s'il n'existait pas.
    pub fn remove(&mut self, id: DocId) -> bool {
        self.docs.remove(&id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Document {
        Document::from_bytes(mega_core::testing::sample_pdf(1, "a"), None).unwrap()
    }

    #[test]
    fn insert_get_remove() {
        let mut r = DocumentRegistry::default();
        let id = r.insert(doc());
        assert!(r.get(id).is_some());
        assert!(r.remove(id));
        assert!(r.get(id).is_none());
        assert!(!r.remove(id));
    }

    #[test]
    fn ids_are_never_reused() {
        let mut r = DocumentRegistry::default();
        let first = r.insert(doc());
        r.remove(first);
        let second = r.insert(doc());
        assert_ne!(first, second);
    }
}
