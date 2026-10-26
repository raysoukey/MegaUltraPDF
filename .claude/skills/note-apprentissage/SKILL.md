---
name: note-apprentissage
description: Rédige la note d'apprentissage de fin de lot (docs/apprentissage/lot-N.md) à partir des commits du lot, et coche le lot dans ROADMAP. Utiliser à la fin d'un lot, quand l'utilisateur dit « /note-apprentissage », « note du lot », « qu'est-ce que j'ai appris ».
---

# Note d'apprentissage de fin de lot

Lot : `$ARGUMENTS` (sinon le lot 🚧 de `docs/ROADMAP.md`).

1. Relire les commits du lot (`git log --oneline <base>..HEAD` et les diffs utiles), la spec
   et le plan du lot, et tous les commentaires `[Apprentissage]` ajoutés pendant le lot.
2. Écrire `docs/apprentissage/lot-N.md` (≤ 200 lignes), en français :
   - **Ce que le lot a construit** (5 lignes, côté utilisateur).
   - **Concepts Rust rencontrés**, du plus simple au plus avancé. Pour chacun : une phrase
     d'explication, où le lire (`fichier:ligne`), et le lien vers
     `docs/apprentissage/concepts/<slug>.md` s'il existe (sinon proposer `/concept`).
   - **Décisions de conception** prises pendant le lot et pourquoi (renvoyer aux ADR).
   - **Pièges rencontrés** pendant le développement (vrais bugs, messages du compilateur,
     comportements de PDFium ou de Tauri) et comment ils ont été résolus.
   - **Exercices de lecture** : 3 endroits du code à relire, chacun avec une question.
   - **Prérequis pour le lot suivant** : les concepts qu'il vaut mieux avoir compris.
3. Mettre à jour `docs/apprentissage/README.md` (index) et cocher le lot ✅ dans
   `docs/ROADMAP.md`.
4. Commiter : `docs(learning): add learning note for batch N`.

La note s'adresse à l'utilisateur (apprenant, quelques tutoriels Rust). Pas de jargon non
expliqué ; chaque affirmation technique renvoie à une ligne de code du dépôt.
