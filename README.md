# MegaUltraPDF

Lecteur et éditeur PDF libre, sans publicité ni télémétrie, écrit en Rust.
Le projet est aussi un support d'apprentissage de Rust : le code est commenté en français
et chaque lot de fonctionnalités s'accompagne d'une note d'apprentissage (`docs/apprentissage/`).

- Moteur PDF : PDFium (celui de Chrome) via `pdfium-render`, piloté exclusivement par `mega-core`.
- Interface : Tauri 2 (backend Rust) + Svelte 5 / TypeScript, sur le WebView2 du système.
- Licence : GPL-3.0-or-later.

## État (voir `docs/ROADMAP.md`)

| Lot | Contenu | État |
|---|---|---|
| 0 | Fondations : cœur Rust, CLI `info`/`render`, application qui affiche la page 1, outillage | 🚧 |
| 1 | Lecteur complet (zoom, navigation, recherche, sélection, onglets) | ⬜ |
| 2 | Pages et documents (fusion, scission, rotation, annuler/rétablir) | ⬜ |
| 3 → 7 | Annotations, formulaires, édition de contenu, export/sécurité, finition | ⬜ |

## Prérequis (développement)

- **Rust stable** via [rustup](https://rustup.rs) (`rustc`, `cargo`, `rustfmt`, `clippy`).
- **Visual Studio 2022** avec la charge « Développement Desktop C++ » (ou les Build Tools 2022
  avec le même composant) : fournit l'éditeur de liens MSVC utilisé par Rust sous Windows.
- **Node.js ≥ 22** et **pnpm ≥ 9** pour l'interface (Tauri 2 + Svelte).
- **WebView2** (inclus dans Windows 11).
- **GitHub CLI** (`gh`), optionnel, pour les pull requests et la CI.
- Sous Linux : les dépendances Tauri 2 (`libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`,
  `librsvg2-dev`, `patchelf`, `build-essential`, `libssl-dev`).

Installation rapide sous Windows (winget) :

```powershell
winget install --id Rustlang.Rustup --exact
winget install --id GitHub.cli --exact
```

## Démarrage rapide

```bash
cargo xtask fetch-pdfium            # bibliothèque PDFium épinglée, somme de contrôle vérifiée
pnpm --dir app install              # dépendances du frontend
cargo test --workspace              # tests Rust
pnpm --dir app tauri dev -- -- fixtures/simple.pdf   # lance l'application sur un fichier
```

## Ligne de commande

```bash
cargo run -p mega-cli -- info fixtures/simple.pdf
cargo run -p mega-cli -- render fixtures/simple.pdf --page 1 --scale 2 --out page1.png
cargo run -p mega-cli -- info fixtures/encrypted.pdf --password secret
```

## Organisation du dépôt

```
crates/mega-core/   logique PDF (seul crate qui parle à PDFium)
crates/mega-cli/    binaire `mega`
app/src-tauri/      backend Tauri (commandes, protocole mega://, registre)
app/src/            frontend SvelteKit (présentation seulement)
xtask/              tâches de dev (fetch-pdfium, gen-fixtures)
fixtures/           PDF de test
docs/               feuille de route, ADR, notes d'apprentissage, specs et plans
.claude/            environnement Claude Code (règles, hooks, compétences, agents)
```

Contribuer : voir `CONTRIBUTING.md`. Règles de travail : `CLAUDE.md`.
