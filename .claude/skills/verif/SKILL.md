---
name: verif
description: Exécute toutes les vérifications du projet (fmt, clippy, tests Rust, rustdoc, svelte-check, eslint, vitest, prettier) et affiche un tableau ✔/✘. Utiliser avant de déclarer un travail terminé, avant un commit important, ou quand l'utilisateur dit « /verif », « vérifie tout », « est-ce que tout passe ».
---

# Vérification complète

Exécuter, dans l'ordre, depuis la racine du dépôt (ajouter `~/.cargo/bin` au PATH si besoin) :

| # | Commande | Ce qu'elle garantit |
|---|---|---|
| 1 | `cargo fmt --all --check` | formatage Rust |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | lints Rust, zéro avertissement |
| 3 | `cargo test --workspace` | tests Rust (core, cli, app, xtask) |
| 4 | `cargo doc -p mega-core --no-deps` | docs `///` valides (zéro warning) |
| 5 | `pnpm --dir app check` | types TS et Svelte |
| 6 | `pnpm --dir app lint` | eslint |
| 7 | `pnpm --dir app test` | vitest |
| 8 | `pnpm --dir app format:check` | prettier |

Pour chaque ligne, capturer le code de sortie et les 20 dernières lignes de sortie en cas
d'échec. Ne pas s'arrêter au premier échec : tout exécuter, puis afficher le tableau
récapitulatif avec ✔ / ✘ et, sous le tableau, l'extrait d'erreur de chaque ✘.

Règles :
- Si une ligne est ✘, **ne jamais** déclarer le travail « fini ». Proposer la correction
  (ou la faire si l'utilisateur l'a demandé) puis relancer `/verif`.
- Si `cargo test` échoue avec `LibraryNotFound`, lancer `cargo xtask fetch-pdfium` et relancer.
- Si le dépôt a un remote GitHub et que `gh` est connecté, terminer par `gh run list -L 3`
  pour rappeler l'état de la CI.
