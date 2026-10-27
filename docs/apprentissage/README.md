# Notes d'apprentissage

Ce dossier accompagne l'apprentissage de Rust à travers le projet. Une note par lot, et une
fiche par concept. Les commentaires `[Apprentissage]` dans le code renvoient ici.

## Notes de lot

- [Lot 0 — Fondations et environnement](lot-0.md)
- [Lot 1 — Lecteur](lot-1.md)
- [Lot 2 — Pages et documents](lot-2.md)
- [Lot 3 — Annotations](lot-3.md)
- [Lot 4 — Formulaires et signature visuelle](lot-4.md)
- [Lot 5 — Édition de contenu](lot-5.md)
- [Lot 6 — Export, impression, sécurité](lot-6.md)
- [Lot 7 — Finition et distribution](lot-7.md)
- [Lot 8 — OCR et signature numérique](lot-8.md)
- [Lot 9 — Consolidation](lot-9.md)

## Fiches concept (`/concept <nom>` pour en ajouter une)

- [`Result` et l'opérateur `?`](concepts/result-et-operateur-point-d-interrogation.md)
- [`thiserror` : la cause n'est pas le message](concepts/thiserror-source-vs-display.md)
- [Ownership et emprunts](concepts/ownership-et-emprunts.md)
- [`OnceLock` : initialiser une fois, lire partout](concepts/oncelock.md)
- [`Mutex` et état partagé](concepts/mutex-et-etat-partage.md)
- [Itérateurs et fermetures](concepts/iterateurs-et-fermetures.md)
- [Canaux et patron acteur](concepts/canaux-et-acteur.md)
- [Traits et objets de trait (`dyn`)](concepts/traits-et-objets-de-trait.md)
- [`enum` riches et `match` exhaustif](concepts/enums-et-match-exhaustif.md)
- [Pattern builder](concepts/pattern-builder.md)
- [`Drop` et ressources natives](concepts/drop-et-ressources-natives.md)
- [Matrices et transformations : changer de repère](concepts/matrices-et-transformations.md)
- [Flux de contenu PDF : lire et écrire le programme d'une page](concepts/flux-de-contenu-pdf.md)
- [`Read`, `Write` et `Cursor` : des octets sans se soucier d'où ils viennent](concepts/read-write-et-cursor.md)
- [Features Cargo et dépendances : n'embarquer que ce qui sert](concepts/features-cargo-et-dependances.md)
- [Profils Cargo et distribution : du `cargo run` au paquet installé](concepts/profils-cargo-et-distribution.md)
- [Complexité et collections : choisir `Vec`, `HashMap` ou `BTreeSet`](concepts/complexite-et-collections.md)
