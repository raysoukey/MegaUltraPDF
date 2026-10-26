---
name: concept
description: Mini-leçon sur un concept Rust (ownership, lifetimes, traits, OnceLock, Result, itérateurs, Mutex…) illustrée uniquement avec le code de ce dépôt, enregistrée dans docs/apprentissage/concepts/. Utiliser quand l'utilisateur dit « /concept <nom> », « c'est quoi les lifetimes », « explique-moi les traits ».
---

# Mini-leçon sur un concept Rust

Concept : `$ARGUMENTS`. Sans argument, proposer les concepts déjà présents dans le code mais
pas encore documentés (comparer `docs/apprentissage/concepts/` aux commentaires
`[Apprentissage]` trouvés par `grep -rn "Apprentissage" crates app/src-tauri`).

1. Chercher dans le dépôt **tous** les endroits où le concept est utilisé
   (`crates/`, `app/src-tauri/`, `xtask/`). La leçon ne doit citer que du code du projet,
   avec `fichier:ligne`. Pas d'exemple inventé, sauf pour montrer un contre-exemple
   (clairement marqué « ne compile pas » ou « mauvaise idée »).
2. Écrire `docs/apprentissage/concepts/<slug>.md` (≤ 120 lignes) :
   - **L'idée en 3 phrases** (pourquoi Rust a ce concept, quel problème il résout).
   - **Dans ce projet** : 2 à 4 extraits commentés ligne par ligne.
   - **Ce qui se passerait sans** : l'erreur de compilation ou le bug évité.
   - **Erreurs fréquentes** et comment lire le message du compilateur.
   - **Pour aller plus loin** : chapitre du Rust Book correspondant (lien), sans plus.
   - **Exercice de lecture** : un endroit du code à relire avec une question.
3. Ajouter une ligne dans `docs/apprentissage/README.md` (index des concepts).
4. Afficher la leçon dans la réponse et commiter (`docs(learning): add concept <slug>`).

Niveau : quelques tutoriels Rust suivis. Français, phrases courtes, un seul concept par leçon.
