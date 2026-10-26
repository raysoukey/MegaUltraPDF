---
name: lot
description: Démarre le lot N de MegaUltraPDF (spec du lot, puis plan d'implémentation, puis exécution). Utiliser quand l'utilisateur dit « /lot N », « on attaque le lot N », « lot suivant ».
---

# Démarrer un lot

Argument : le numéro du lot (`$ARGUMENTS`). Sans argument, prendre le premier lot ⬜ de
`docs/ROADMAP.md`.

1. Lire `docs/ROADMAP.md`, la section du lot dans
   `docs/superpowers/specs/2026-10-26-megaultrapdf-design.md` (§5) et la note d'apprentissage
   du lot précédent (`docs/apprentissage/lot-<N-1>.md`) pour connaître le niveau atteint.
2. Si le périmètre du lot est flou ou si l'utilisateur veut le discuter, invoquer
   `superpowers:brainstorming`. Sinon, rédiger directement la spec du lot :
   `docs/superpowers/specs/<date>-lot-N-<nom>.md` avec : intention, périmètre exact (ce qui
   entre, ce qui n'entre pas), API publiques ajoutées à `mega-core` / commandes Tauri /
   types `api.ts`, concepts Rust travaillés (avec le niveau de l'apprenant), fixtures
   nécessaires, checklist de validation manuelle de fin de lot.
3. Faire valider la spec par l'utilisateur.
4. Invoquer `superpowers:writing-plans` pour produire
   `docs/superpowers/plans/<date>-lot-N-<nom>.md`. Chaque tâche : tests d'abord, signatures
   exactes, commandes de vérification, commit.
5. Marquer le lot 🚧 dans `docs/ROADMAP.md` et commiter spec + plan.
6. Proposer l'exécution (native ou par sous-agents) et, une fois choisie, l'enchaîner.

Rappels : respecter les frontières de `CLAUDE.md` ; chaque tâche se termine par le résumé
« Ce que vous venez de voir » ; le lot se termine par `/note-apprentissage` puis `/verif`.
