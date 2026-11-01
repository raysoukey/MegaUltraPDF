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

## Contrôle de conformité par des outils externes

`cargo test -p mega-core --test conformance` (`crates/mega-core/tests/conformance/`) fait relire
chaque fichier que nous écrivons par d'autres moteurs que PDFium. Ces outils ne sont pas dans le
dépôt : chacun se désigne par une variable d'environnement, et le contrôle correspondant est
**sauté** si elle manque.

| Variable | Ce qu'elle active |
|---|---|
| `MEGA_QPDF` | `qpdf --check` sur chaque sortie (sinon `qpdf` du `PATH`). S'il est introuvable, chaque test affiche « AVERTISSEMENT : qpdf introuvable, contrôle de conformité SAUTÉ (poser MEGA_QPDF) » et passe ; `MEGA_REQUIRE_QPDF=1` (posé par la CI) le rend obligatoire. |
| `MEGA_MUTOOL` | relecture par MuPDF (`mutool info` : code 0, aucune « error »). |
| `MEGA_PDFJS_CHECK` | relecture par pdf.js : chemin de `scripts/audit/pdfjs-check.mjs`, lancé par `node` (`pdfjs-dist` installé dans `tools/pdfjs`, voir l'en-tête du script). |
| `MEGA_VERAPDF` | veraPDF (`verapdf` ou `verapdf.bat`) : un filigrane et une numérotation gardent la conformité PDF/A et PDF/UA du corpus (exige aussi `MEGA_CORPUS`). |
| `MEGA_CORPUS` | dossier du corpus de PDF réels (`fixtures/corpus`, récupéré par `bash scripts/audit/fetch-corpus.sh`) : tout s'ouvre sans panique, et chaque fichier tourné puis réenregistré passe qpdf si l'original le passait. |

Sous Windows, dans Git Bash, depuis la racine du dépôt (chemins de ce poste, à adapter) :

```
export MEGA_QPDF='C:\Program Files\qpdf 12.4.2\bin\qpdf.exe'
export MEGA_REQUIRE_QPDF=1
export MEGA_MUTOOL="$PWD/tools/dl/mupdf/mutool.exe"
export MEGA_PDFJS_CHECK="$PWD/scripts/audit/pdfjs-check.mjs"
export MEGA_VERAPDF="$PWD/tools/verapdf/verapdf.bat"
export MEGA_CORPUS="$PWD/fixtures/corpus"
cargo test -p mega-core --test conformance -j 2 -- --nocapture
```

En PowerShell : `$env:MEGA_QPDF = 'C:\Program Files\qpdf 12.4.2\bin\qpdf.exe'`, etc.
`cargo test … -- --include-ignored` lance aussi les exigences pas encore tenues (marquées
`#[ignore]` avec le constat qui les bloque).

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
