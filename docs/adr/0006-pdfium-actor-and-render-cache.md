# ADR-0006 — Acteur PDFium, file priorisée, cache de rendu et générations

Date : 2026-10-26 · Statut : accepté (Lot 1)

## Contexte

Le Lot 0 sérialisait l'accès à PDFium par un verrou de processus (ADR-0004) et exécutait
les commandes Tauri de façon synchrone : un rendu long figeait l'interface, les vignettes et
les pages visibles se disputaient le même verrou dans l'ordre d'arrivée, et un zoom rapide
produisait des rendus inutiles. Le lecteur du Lot 1 (onglets, défilement continu virtualisé,
vignettes, recherche) a besoin d'un rendu **asynchrone, ordonné et annulable**.

## Décision

- **Un acteur** (`mega_core::service`, thread `pdfium-actor`) possède tous les documents
  ouverts. Il reçoit des messages (`enum Request`, une variante par opération, chacune avec
  son canal de réponse `sync_channel(1)`) et répond ; personne d'autre ne touche à PDFium.
  Le verrou `engine::guard()` est conservé par sécurité mais n'est plus contendu.
- **File priorisée** (`BinaryHeap`) : `Visible > Prefetch > Thumbnail`, puis ancienneté.
  L'acteur aspire toutes les requêtes déjà arrivées avant de traiter la plus prioritaire :
  une page visible envoyée après dix vignettes passe devant elles. Les opérations qui ne
  sont pas des rendus (ouvrir, fermer, chercher…) sont traitées comme `Visible`.
- **Générations** : chaque document porte un compteur **possédé par le frontend**, qui
  l'incrémente de façon synchrone à chaque changement de zoom ou de rotation et en informe
  l'acteur (`set_generation`, qui ne recule jamais). Un rendu dont la génération est plus
  ancienne que celle connue est refusé sans travail (`CoreError::Cancelled`, HTTP 409 sur
  `mega://`) ; un rendu plus récent fait avancer le compteur de l'acteur. Le cache n'est pas
  vidé : sa clé contient déjà l'échelle et la rotation, et revenir à un zoom précédent
  réutilise les images. (Révision après relecture : une génération attribuée par l'acteur
  créait une course entre deux zooms rapprochés.)
- **Cache LRU de PNG** (`PngCache`, 256 Mo par défaut), clé `(document, page, échelle × 100,
  rotation)`, images partagées en `Arc<Vec<u8>>` entre le cache et les réponses.
- **Handle bloquant** (`ServiceHandle`, clonable) ; le backend Tauri l'appelle depuis
  `tauri::async_runtime::spawn_blocking` dans des commandes `async`, et le protocole
  `mega://` est enregistré en mode asynchrone (`UriSchemeResponder`).

## Alternatives écartées

- `Arc<Mutex<Document>>` partagé et commandes `async` : compile, mais sérialise sans
  ordonner ni annuler, et garde PDFium sur des threads arbitraires.
- Un pool de plusieurs acteurs PDFium : PDFium n'est pas conçue pour un usage concurrent,
  même avec des instances séparées dans le même processus.
- `tokio::sync::mpsc` et `oneshot` : auraient lié `mega-core` à `tokio` ; les canaux de la
  bibliothèque standard suffisent, et le CLI reste synchrone.

## Conséquences

- Les rendus ne sont jamais parallèles (une seule PDFium) ; l'ordre et l'annulation
  comptent plus que le parallélisme pour la fluidité ressentie.
- Une recherche sur tout le document occupe l'acteur ; les rendus attendent. À surveiller
  sur de très gros documents (découpage par lots au Lot 2 si nécessaire).
- Le frontend doit toujours envoyer la génération courante et ignorer les 409.
- Tests : `crates/mega-core/tests/service.rs` (priorité, cache, annulation, fermeture
  pendant usage, handles multi-threads).
