# Contribuer à MegaUltraPDF

## Mise en route

1. Prérequis : voir `README.md` (Rust stable, Visual Studio 2022 avec C++ sous Windows,
   Node ≥ 22, pnpm ≥ 9, WebView2).
2. `cargo xtask fetch-pdfium` : télécharge la bibliothèque PDFium épinglée dans `vendor/pdfium/`.
3. `pnpm --dir app install`.
4. `cargo test --workspace` puis `pnpm --dir app tauri dev -- -- fixtures/simple.pdf`.

## Vérifications avant tout commit

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc -p mega-core --no-deps
pnpm --dir app check && pnpm --dir app lint && pnpm --dir app test && pnpm --dir app format:check
```

La CI (`.github/workflows/ci.yml`) exécute ces commandes sur Windows, Linux et macOS,
précédées de `cargo xtask fetch-pdfium` et suivies de `pnpm --dir app tauri build --debug --no-bundle`.

## Conventions

- **Commits** : Conventional Commits en anglais (`feat(core): …`, `fix(cli): …`, `docs: …`,
  `chore: …`), un sujet par commit.
- **Langues** : identifiants et commits en anglais ; commentaires, docs et interface en français.
- **Licence** : GPL-3.0-or-later ; en-tête SPDX en première ligne de chaque fichier source.
- **Sécurité** : `#![forbid(unsafe_code)]` partout ; aucun appel réseau à l'exécution ;
  les données venant d'un PDF sont non fiables.
- **Architecture** : seul `crates/mega-core` parle à PDFium ; le backend Tauri est une fine
  couche ; le frontend ne contient que de la présentation. Voir `CLAUDE.md` et `docs/adr/`.
- **Tests d'abord** : tout comportement nouveau arrive avec un test qui échouait avant.
- **Fixtures** : `cargo xtask gen-fixtures` régénère `fixtures/simple.pdf` ;
  `fixtures/encrypted.pdf` a été produit avec
  `qpdf --encrypt --user-password=secret --owner-password=secret --bits=256 -- simple.pdf encrypted.pdf`.

## Travailler avec Claude Code

Le dépôt embarque des règles (`CLAUDE.md`), des hooks (formatage automatique, vérification à
la fin de chaque tâche), des compétences (`/lot`, `/explique`, `/concept`,
`/note-apprentissage`, `/verif`, `/pdf-spec`) et des agents de relecture
(`rust-reviewer`, `pdf-expert`, `ui-reviewer`) dans `.claude/`.
