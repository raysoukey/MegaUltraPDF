# ADR-0005 — Politique de gestion des erreurs

Date : 2026-10-26 · Statut : accepté

## Contexte

Le Lot 0 a établi un schéma d'erreurs (enum `CoreError` avec `thiserror`, `anyhow` dans le
CLI, `AppError` sérialisée côté Tauri) et rencontré deux pièges réels : un message qui
répétait sa cause, et un code d'erreur PDFium identique pour deux situations différentes.
Les lots suivants ajouteront des threads (acteur), des commandes `async` et de nombreuses
opérations d'édition. Il faut une règle stable avant que le nombre de variantes grandisse.

## Décision

1. **Une seule enum typée par bibliothèque.** `mega-core` expose `CoreError` ; chaque cas
   que l'application doit distinguer est une **variante**, jamais une chaîne à analyser.
   La variante fourre-tout `Pdfium(String)` reste un filet pour l'inattendu, pas un raccourci.
2. **Convertir aux frontières, décider aux extrémités.** PDFium → `CoreError` dans le cœur
   (`impl From`), `CoreError` → `AppError { kind, message }` à l'entrée de Tauri, `anyhow`
   dans le CLI. Entre les deux, le code ne fait que `?`. Seuls `main` et `AppError`
   choisissent quoi afficher.
3. **Garder la cause sans la répéter.** `#[from]`/`#[source]` conservent la cause ; le
   message `Display` ne la recopie pas ; la chaîne complète est produite à l'affichage
   (`mega_core::error::display_chain`, `{e:#}` d'`anyhow`).
4. **Ajouter du contexte là où l'information se perd** (`with_context` avec le chemin, les
   bornes, l'index), pas partout.
5. **Pas de panique dans le code de production.** `unwrap`, `expect`, `panic!`, `todo!`,
   `unimplemented!` sont interdits par lint dans `mega-core`, `mega-cli` et `mega-app`
   (`clippy::unwrap_used`, `clippy::expect_used`, `clippy::panic`, `clippy::todo`,
   `clippy::unimplemented`). Un invariant qui justifie vraiment un `expect` est marqué
   `#[allow(clippy::expect_used)]` avec, en commentaire, pourquoi il ne peut pas échouer.
   Les tests et le module `testing` en sont exemptés.
6. **Un test par variante**, avec une fixture ou un générateur qui la provoque.
7. **Erreurs prêtes à traverser les threads** : `Send + Sync + 'static`, aucune référence
   empruntée dans une variante (contrainte du patron acteur du Lot 1).
8. **Données non fiables** : tout texte issu d'un PDF passe par une neutralisation avant
   affichage terminal (`display_safe`) ; côté interface, le texte est affiché comme texte,
   jamais interprété.

## Conséquences

- Ajouter une erreur = ajouter une variante + son test + sa catégorie `ErrorKind` si le
  frontend doit la distinguer, et une chaîne dans `strings.ts`.
- Les lints échouent la CI (`-D warnings`) si un `unwrap` se glisse dans la production.
- Les `expect` restants sont explicitement listés par `#[allow]` et relus à chaque lot.
