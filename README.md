# MegaUltraPDF

Lecteur et éditeur PDF libre, sans publicité ni télémétrie, écrit en Rust.
Le projet est aussi un support d'apprentissage de Rust : le code est commenté en français
et chaque lot de fonctionnalités s'accompagne d'une note d'apprentissage (`docs/apprentissage/`).

## Prérequis (développement)

- **Rust stable** via [rustup](https://rustup.rs) (`rustc`, `cargo`, `rustfmt`, `clippy`).
- **Visual Studio 2022** avec la charge « Développement Desktop C++ » (ou les Build Tools 2022
  avec le même composant) : fournit l'éditeur de liens MSVC utilisé par Rust sous Windows.
- **Node.js ≥ 22** et **pnpm ≥ 9** pour l'interface (Tauri 2 + Svelte).
- **WebView2** (inclus dans Windows 11).
- **GitHub CLI** (`gh`), optionnel, pour les pull requests et la CI.
- Après le clonage : `cargo xtask fetch-pdfium` télécharge la bibliothèque PDFium
  (version épinglée, somme de contrôle vérifiée) dans `vendor/pdfium/`.

Installation rapide sous Windows (winget) :

```powershell
winget install --id Rustlang.Rustup --exact
winget install --id GitHub.cli --exact
```
