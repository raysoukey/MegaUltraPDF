# Lot 0 — Fondations et environnement : plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Un dépôt MegaUltraPDF compilable de bout en bout (Rust + Tauri 2 + Svelte), capable d'afficher la page 1 d'un PDF, avec l'outillage installé, la CI verte sur GitHub et l'environnement Claude Code pédagogique en place.

**Architecture:** Workspace Cargo avec `mega-core` (toute la logique PDF, via `pdfium-render`), `mega-cli` (banc d'essai en ligne de commande), `xtask` (tâches de dev) et `app/src-tauri` (fine couche de commandes Tauri + protocole `mega://` servant des PNG). Le frontend Svelte 5 dans `app/src` n'a que de la présentation. Une seule instance `Pdfium` globale (`OnceLock`) évite les problèmes de lifetime ; l'accès concurrent est sérialisé par la feature `thread_safe` de `pdfium-render` (le patron acteur arrive au Lot 1 avec le rendu asynchrone et le cache).

**Tech Stack:** Rust stable (MSVC), `pdfium-render` 0.9.4 (features par défaut : `pdfium_latest` = API 7881, `image_latest`, `thread_safe`), binaires PDFium bblanchon `chromium/7881`, `thiserror` 2, `anyhow` 1, `clap` 4, `image` 0.25, `tauri` 2.12, `tauri-plugin-dialog` 2.8, Svelte 5.57, Vite, TypeScript strict, Vitest, pnpm 11, Node 22.

**Spec:** `docs/superpowers/specs/2026-10-26-megaultrapdf-design.md`

## Global Constraints

- Licence **GPL-3.0-or-later** ; en-tête SPDX `// SPDX-License-Identifier: GPL-3.0-or-later` en première ligne de chaque fichier `.rs`, `.ts`, `.svelte` (dans `<!-- -->` pour Svelte).
- Identifiants et messages de commit en **anglais** ; commentaires, docs `///`, notes et interface en **français**.
- Tout élément `pub` de `mega-core` porte une doc `///` en français qui dit *pourquoi* il existe.
- Un concept Rust nouveau pour l'apprenant est signalé par `// [Apprentissage] …` avec renvoi à `docs/apprentissage/`.
- `app/src-tauri` et `mega-cli` n'importent **jamais** `pdfium_render` ; seul `mega-core` le fait.
- Aucune chaîne visible en dur dans les composants Svelte : tout passe par `app/src/lib/strings.ts`.
- Aucun appel réseau à l'exécution de l'application. Le seul téléchargement est `cargo xtask fetch-pdfium`, en développement.
- `unsafe` interdit (`#![forbid(unsafe_code)]` dans chaque crate).
- Commandes canoniques : `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm --dir app check`, `pnpm --dir app test`, `pnpm --dir app lint`.
- Fin de ligne : LF partout (`.gitattributes` `* text=auto eol=lf`).

## Review Focus

1. **PDF introuvable ou illisible** (chemin inexistant, fichier qui n'est pas un PDF) : `mega-cli info` et la commande Tauri `open_document` renvoient une erreur lisible en français, jamais un panic. Test dans la Tâche 4 (`open_rejects_non_pdf_bytes`) et la Tâche 5 (`info_reports_missing_file`).
2. **PDF protégé par mot de passe sans mot de passe fourni** : erreur dédiée `CoreError::PasswordRequired`, pas une erreur générique. Test dans la Tâche 4 (`open_encrypted_without_password_is_password_required`).
3. **DLL PDFium absente** (l'utilisateur n'a pas lancé `fetch-pdfium`) : message qui dit exactement quoi lancer. Test dans la Tâche 4 (`missing_library_error_mentions_xtask`).
4. **Index de page hors bornes** dans `mega://page/<id>/<n>` ou `render_page` : erreur `CoreError::PageOutOfRange`, réponse HTTP 404 côté protocole. Tests Tâche 4 (`render_out_of_range_page_fails`) et Tâche 7 (`parse_rejects_bad_index`, test de la réponse 404).
5. **Chemin avec espaces ou accents** (ex. `C:\Users\labre\Mes documents\résumé.pdf`) : ouverture correcte via le dialogue et via le CLI. Test Tâche 5 (`info_handles_unicode_path`).

---

### Tâche 1 : Outillage de la machine

**Files:**
- Create: `README.md` (section « Prérequis » seulement ; le reste vient en Tâche 10)

**Interfaces:**
- Produces: `cargo`, `rustc`, `rustfmt`, `clippy`, `gh` disponibles dans le PATH ; extensions VS Code installées.

État constaté le 2026-10-26 : Visual Studio Community 2022 17.14 **avec** le composant `VC.Tools.x86.x64` et le SDK Windows 10.0.26100 sont présents (pas besoin des Build Tools séparés) ; Node v22.21.0, pnpm 11.18.0, WebView2 155 et winget 1.29 sont présents ; **Rust et `gh` sont absents** ; aucune extension VS Code pertinente.

- [ ] **Step 1 : Installer rustup (invite UAC possible)**

Run (PowerShell) :
```powershell
winget install --id Rustlang.Rustup --exact --accept-source-agreements --accept-package-agreements
```
Expected : « Installé avec succès ». Puis ouvrir un **nouveau** shell (le PATH change).

- [ ] **Step 2 : Vérifier la toolchain**

Run : `rustc --version; cargo --version; rustup component list --installed`
Expected : `rustc 1.9x.x`, `cargo 1.9x.x`, et dans la liste `clippy-x86_64-pc-windows-msvc`, `rustfmt-x86_64-pc-windows-msvc`, `rust-std-x86_64-pc-windows-msvc`. Si clippy ou rustfmt manquent : `rustup component add clippy rustfmt`.

- [ ] **Step 3 : Vérifier que l'éditeur de liens MSVC est trouvé**

Run : dans un dossier temporaire du scratchpad, `cargo new --bin linkcheck; cd linkcheck; cargo run`
Expected : `Hello, world!`. Une erreur `link.exe not found` signifierait que la charge C++ de Visual Studio est incomplète ; la corriger avec `winget install --id Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"` puis recommencer.

- [ ] **Step 4 : Installer GitHub CLI**

Run : `winget install --id GitHub.cli --exact --accept-source-agreements --accept-package-agreements`
Expected : « Installé avec succès » ; `gh --version` dans un nouveau shell affiche `gh version 2.x`.

- [ ] **Step 5 : Installer les extensions VS Code**

Run :
```powershell
foreach ($e in 'rust-lang.rust-analyzer','vadimcn.vscode-lldb','tamasfe.even-better-toml','svelte.svelte-vscode','esbenp.prettier-vscode','dbaeumer.vscode-eslint') { code --install-extension $e --force }
```
Expected : six lignes « was successfully installed ».

- [ ] **Step 6 : Écrire la section Prérequis du README**

Créer `README.md` avec le titre `# MegaUltraPDF`, une phrase de présentation (lecteur et éditeur PDF libre, sans publicité ni télémétrie, en Rust), et une section `## Prérequis (développement)` listant : Rust stable via rustup, Visual Studio 2022 avec « Développement Desktop C++ » (ou Build Tools), Node.js ≥ 22 et pnpm ≥ 9, WebView2 (inclus dans Windows 11), GitHub CLI (optionnel), et la commande `cargo xtask fetch-pdfium` (expliquée en Tâche 3). Pas de commit : le dépôt n'a pas encore de workspace ; ce fichier part avec la Tâche 2.

---

### Tâche 2 : Squelette du workspace

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `.gitattributes`, `.gitignore` (modifier : déjà `.remember/`), `LICENSE`, `crates/mega-core/Cargo.toml`, `crates/mega-core/src/lib.rs`, `crates/mega-cli/Cargo.toml`, `crates/mega-cli/src/main.rs`, `xtask/Cargo.toml`, `xtask/src/main.rs`, `.cargo/config.toml`

**Interfaces:**
- Produces: workspace `members = ["crates/*", "xtask"]` (le membre `app/src-tauri` est ajouté en Tâche 6) ; alias `cargo xtask` ; `[workspace.package]` avec `edition = "2024"`, `license = "GPL-3.0-or-later"`, `rust-version = "1.85"` ; `[workspace.dependencies]` pour `thiserror = "2"`, `anyhow = "1"`, `serde = { version = "1", features = ["derive"] }`.

- [ ] **Step 1 : Créer les manifestes**

`Cargo.toml` racine : `[workspace] resolver = "3"`, membres ci-dessus, `[workspace.package]`, `[workspace.dependencies]`, et `[profile.release] lto = "thin", codegen-units = 1, strip = true`.
`rust-toolchain.toml` : `[toolchain] channel = "stable" components = ["rustfmt", "clippy"]`.
`.cargo/config.toml` : `[alias] xtask = "run -q -p xtask --"`.
`crates/mega-core/Cargo.toml` : lib, nom `mega-core`, `[lints.rust] unsafe_code = "forbid"`, dépendances `thiserror.workspace = true`.
`crates/mega-cli/Cargo.toml` : bin `mega`, dépendances `mega-core = { path = "../mega-core" }`, `anyhow.workspace = true`.
`xtask/Cargo.toml` : bin `xtask`, `anyhow.workspace = true`.
`.gitattributes` : `* text=auto eol=lf`.
`.gitignore` : `.remember/`, `/target`, `/vendor/pdfium/`, `node_modules/`, `app/dist/`, `app/src-tauri/gen/`, `.DS_Store`.
`LICENSE` : texte intégral GPL-3.0 (https://www.gnu.org/licenses/gpl-3.0.txt).

- [ ] **Step 2 : Fichiers source minimaux**

`crates/mega-core/src/lib.rs` : en-tête SPDX, `#![forbid(unsafe_code)]`, doc de crate `//!` en français (« Cœur de MegaUltraPDF : toute la logique PDF, sans interface »), et `pub const VERSION: &str = env!("CARGO_PKG_VERSION");`.
`crates/mega-cli/src/main.rs` : `fn main() { println!("mega {}", mega_core::VERSION); }`.
`xtask/src/main.rs` : `fn main() -> anyhow::Result<()> { println!("xtask: no command yet"); Ok(()) }`.

- [ ] **Step 3 : Vérifier**

Run : `cargo build --workspace; cargo run -p mega-cli; cargo xtask; cargo fmt --all --check; cargo clippy --workspace --all-targets -- -D warnings`
Expected : build OK, `mega 0.1.0`, `xtask: no command yet`, fmt et clippy sans sortie.

- [ ] **Step 4 : Commit**

```bash
git add -A
git commit -m "chore: bootstrap cargo workspace (mega-core, mega-cli, xtask)"
```

---

### Tâche 3 : `cargo xtask fetch-pdfium`

**Files:**
- Create: `xtask/src/pdfium.rs`, `xtask/pdfium.lock.toml`
- Modify: `xtask/src/main.rs`, `xtask/Cargo.toml`

**Interfaces:**
- Produces: la bibliothèque PDFium dans `vendor/pdfium/<target-triple>/` (`pdfium.dll` sur Windows, `libpdfium.so` sur Linux, `libpdfium.dylib` sur macOS) ; `vendor/pdfium/VERSION` contenant le tag. `mega-core` (Tâche 4) cherche la bibliothèque à cet endroit.
- Fonctions testables : `pub fn asset_for_target(target: &str) -> Option<&'static Asset>` et `pub fn verify_sha256(bytes: &[u8], expected_hex: &str) -> anyhow::Result<()>`.

Version épinglée (vérifiée le 2026-10-26, release du 2026-06-08) : tag `chromium/7881`, URL `https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F7881/<asset>`.

| target | asset | sha256 | chemin dans l'archive |
|---|---|---|---|
| `x86_64-pc-windows-msvc` | `pdfium-win-x64.tgz` | `73cc0de638ac2095e7445bf56a38200a5b7c7ca0e9f4ba144598f2457377ac08` | `bin/pdfium.dll` |
| `x86_64-unknown-linux-gnu` | `pdfium-linux-x64.tgz` | `1470e21b8b4a3b4ad7f85684e2da11d94f3b69a86d81dee11b9b6709d927ac1d` | `lib/libpdfium.so` |
| `aarch64-apple-darwin` et `x86_64-apple-darwin` | `pdfium-mac-univ.tgz` | `df451a413c3609585e84a4a91110a9bc889cff05fe3b2db0ed817c9e90c3f7d3` | `lib/libpdfium.dylib` |

- [ ] **Step 1 : Écrire les tests**

Dans `xtask/src/pdfium.rs`, module `#[cfg(test)]` :
```rust
#[test]
fn windows_target_maps_to_win_x64_asset() {
    let a = asset_for_target("x86_64-pc-windows-msvc").unwrap();
    assert_eq!(a.file_name, "pdfium-win-x64.tgz");
    assert_eq!(a.library_path_in_archive, "bin/pdfium.dll");
}
#[test]
fn unknown_target_is_none() { assert!(asset_for_target("wasm32-unknown-unknown").is_none()); }
#[test]
fn sha256_mismatch_is_error() {
    assert!(verify_sha256(b"hello", "00").is_err());
    assert!(verify_sha256(b"hello", "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824").is_ok());
}
```

- [ ] **Step 2 : Vérifier qu'ils échouent**

Run : `cargo test -p xtask`
Expected : erreur de compilation (`asset_for_target` inconnu).

- [ ] **Step 3 : Implémenter**

`xtask/Cargo.toml` : ajouter `ureq = "3"`, `sha2 = "0.10"`, `flate2 = "1"`, `tar = "0.4"`, `toml = "0.9"`, `serde.workspace = true`.
`xtask/pdfium.lock.toml` : `tag = "chromium/7881"` puis une table `[[asset]]` par ligne du tableau ci-dessus (`targets = [...]`, `file_name`, `sha256`, `library_path_in_archive`). Le fichier est inclus avec `include_str!` et désérialisé.
`xtask/src/pdfium.rs` :
- `pub struct Asset { pub targets: Vec<String>, pub file_name: String, pub sha256: String, pub library_path_in_archive: String }`
- `pub fn asset_for_target(target: &str) -> Option<&'static Asset>` (lock chargé dans un `OnceLock`).
- `pub fn verify_sha256(bytes: &[u8], expected_hex: &str) -> anyhow::Result<()>`.
- `pub fn fetch(workspace_root: &Path, target: &str) -> anyhow::Result<PathBuf>` : si `vendor/pdfium/<target>/<lib>` existe et `vendor/pdfium/VERSION` == tag, ne rien faire ; sinon télécharger, vérifier, extraire uniquement `library_path_in_archive` vers la destination, écrire `VERSION`. Affiche la progression en français.
`xtask/src/main.rs` : sous-commande `fetch-pdfium` (parsing à la main sur `std::env::args`, pas de clap ici) ; `--target <triple>` optionnel, défaut = triple hôte obtenu via `rustc -vV` (ligne `host:`). Racine du workspace = `env!("CARGO_MANIFEST_DIR")/..`.

- [ ] **Step 4 : Tests verts et exécution réelle**

Run : `cargo test -p xtask; cargo xtask fetch-pdfium; cargo xtask fetch-pdfium`
Expected : 3 tests OK ; première exécution télécharge (~3,7 Mo) et affiche le chemin `vendor\pdfium\x86_64-pc-windows-msvc\pdfium.dll` ; seconde exécution dit « déjà présent ».

- [ ] **Step 5 : Commit**

```bash
git add xtask .gitignore
git commit -m "feat(xtask): add fetch-pdfium with pinned version and sha256 check"
```

---

### Tâche 4 : `mega-core` — moteur, document, rendu

**Files:**
- Create: `crates/mega-core/src/error.rs`, `crates/mega-core/src/engine.rs`, `crates/mega-core/src/document.rs`, `crates/mega-core/src/render.rs`, `crates/mega-core/src/testing.rs`, `crates/mega-core/tests/document.rs`
- Modify: `crates/mega-core/src/lib.rs`, `crates/mega-core/Cargo.toml`

**Interfaces:**
- Produces (tout `pub`, réexporté dans `lib.rs`) :
  - `enum CoreError` (`thiserror`) : `LibraryNotFound { searched: Vec<PathBuf> }` (message : « Bibliothèque PDFium introuvable. Lancez `cargo xtask fetch-pdfium`. Chemins essayés : … »), `PasswordRequired`, `InvalidPdf(String)`, `PageOutOfRange { index: u16, count: u16 }`, `Io(#[from] std::io::Error)`, `Pdfium(String)`.
  - `type CoreResult<T> = Result<T, CoreError>`.
  - `engine::pdfium() -> CoreResult<&'static Pdfium>` (interne au crate : `pub(crate)`), `engine::library_search_paths() -> Vec<PathBuf>` (pub, pour les messages).
  - `struct Document` avec `Document::open(path: impl AsRef<Path>, password: Option<&str>) -> CoreResult<Document>`, `Document::from_bytes(bytes: Vec<u8>, password: Option<&str>) -> CoreResult<Document>`, `fn path(&self) -> Option<&Path>`, `fn title(&self) -> Option<String>`, `fn page_count(&self) -> u16`, `fn page_size(&self, index: u16) -> CoreResult<PageSize>`, `fn page_sizes(&self) -> CoreResult<Vec<PageSize>>`, `fn to_bytes(&self) -> CoreResult<Vec<u8>>`.
  - `#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)] struct PageSize { pub width_pt: f32, pub height_pt: f32 }`.
  - `struct RenderedPage { pub width_px: u32, pub height_px: u32, pub rgba: Vec<u8> }` et `Document::render_page(&self, index: u16, scale: f32) -> CoreResult<RenderedPage>` (scale 1.0 = 72 dpi, donc largeur_px = round(width_pt × scale)), `RenderedPage::to_png(&self) -> CoreResult<Vec<u8>>`.
  - `testing::sample_pdf(pages: u16, title: &str) -> Vec<u8>` (feature `testing`, aussi utilisé par mega-cli et par l'app en tests) : document A4 créé avec PDFium, chaque page portant le texte « Page N » en Helvetica 24 pt.

Décision d'implémentation (choix de Claude, documentée en ADR-0004) : `PdfDocument<'a>` emprunte `Pdfium`. Pour que `Document` soit un type possédé simple, `Pdfium` est une instance globale `static PDFIUM: OnceLock<Pdfium>` ; `Document` contient un `PdfDocument<'static>`. La feature `thread_safe` (par défaut) sérialise les accès.

Recherche de la bibliothèque, dans l'ordre : variable `MEGA_PDFIUM_DIR` ; dossier de l'exécutable ; `<racine workspace>/vendor/pdfium/<triple hôte>/` (racine = `CARGO_MANIFEST_DIR/../..` à la compilation, utile pour `cargo run` et `cargo test`) ; enfin `Pdfium::bind_to_system_library()`.

- [ ] **Step 1 : Dépendances**

`crates/mega-core/Cargo.toml` : `pdfium-render = "0.9.4"`, `image = { version = "0.25", default-features = false, features = ["png"] }`, `serde.workspace = true`, `thiserror.workspace = true` ; `[features] testing = []`.

- [ ] **Step 2 : Écrire les tests d'intégration**

`crates/mega-core/tests/document.rs` (activer `testing` pour les tests via `[dev-dependencies] mega-core = { path = ".", features = ["testing"] }`) :
```rust
use mega_core::{testing::sample_pdf, CoreError, Document};

#[test]
fn opens_bytes_and_reads_metadata() {
    let doc = Document::from_bytes(sample_pdf(3, "Exemple"), None).unwrap();
    assert_eq!(doc.page_count(), 3);
    assert_eq!(doc.title().as_deref(), Some("Exemple"));
    let s = doc.page_size(0).unwrap();
    assert!((s.width_pt - 595.0).abs() < 1.0 && (s.height_pt - 842.0).abs() < 1.0);
}
#[test]
fn open_rejects_non_pdf_bytes() {
    assert!(matches!(Document::from_bytes(b"pas un pdf".to_vec(), None), Err(CoreError::InvalidPdf(_))));
}
#[test]
fn open_reports_missing_file() {
    assert!(matches!(Document::open("n/existe/pas.pdf", None), Err(CoreError::Io(_))));
}
#[test]
fn render_out_of_range_page_fails() {
    let doc = Document::from_bytes(sample_pdf(1, "x"), None).unwrap();
    assert!(matches!(doc.render_page(5, 1.0), Err(CoreError::PageOutOfRange { index: 5, count: 1 })));
}
#[test]
fn render_scale_controls_pixel_size() {
    let doc = Document::from_bytes(sample_pdf(1, "x"), None).unwrap();
    let r = doc.render_page(0, 2.0).unwrap();
    assert_eq!((r.width_px, r.height_px), (1190, 1684));
    assert_eq!(r.rgba.len(), 1190 * 1684 * 4);
    let png = r.to_png().unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
}
#[test]
fn roundtrip_to_bytes_keeps_pages() {
    let doc = Document::from_bytes(sample_pdf(2, "x"), None).unwrap();
    let again = Document::from_bytes(doc.to_bytes().unwrap(), None).unwrap();
    assert_eq!(again.page_count(), 2);
}
#[test]
fn open_encrypted_without_password_is_password_required() {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/encrypted.pdf")).unwrap();
    assert!(matches!(Document::from_bytes(bytes.clone(), None), Err(CoreError::PasswordRequired)));
    assert_eq!(Document::from_bytes(bytes, Some("secret")).unwrap().page_count(), 1);
}
```
Test unitaire dans `engine.rs` : `missing_library_error_mentions_xtask` — construire `CoreError::LibraryNotFound { searched: vec![] }` et vérifier que `to_string()` contient `cargo xtask fetch-pdfium`.

Fixture `fixtures/encrypted.pdf` : PDFium ne sait pas chiffrer ; générer une fois ce fichier d'une page avec mot de passe utilisateur `secret` via `qpdf --encrypt secret secret 256 -- in.pdf fixtures/encrypted.pdf` (installer `qpdf` avec `winget install --id QPDF.QPDF` si absent, puis le désinstaller si on veut) ou, à défaut, avec Python `pypdf` (`-I`). Le fichier est commité (~1 Ko). Le `in.pdf` est `fixtures/simple.pdf` produit au Step 5.

- [ ] **Step 3 : Vérifier l'échec**

Run : `cargo test -p mega-core`
Expected : erreur de compilation (types manquants).

- [ ] **Step 4 : Implémenter**

- `error.rs` : l'enum ci-dessus. Messages `#[error(...)]` en français ; en particulier `PasswordRequired` → « Ce document est protégé par un mot de passe. », `PageOutOfRange` → « Page {index} hors limites (le document a {count} pages). ». Conversion `From<PdfiumError>` : `PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError)` → `PasswordRequired` ; `…FormatError` → `InvalidPdf` ; `…FileError` → `Io` ; sinon `Pdfium(format!("{e:?}"))`. (Vérifier les noms exacts des variantes dans `pdfium_render::error` de la version 0.9.4.)
- `engine.rs` : `library_search_paths()`, `pdfium()` qui essaie chaque chemin avec `Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(dir))`, puis `bind_to_system_library()`, et stocke dans le `OnceLock`. `// [Apprentissage] OnceLock : initialisation paresseuse et sûre entre threads`.
- `document.rs` : `Document { inner: PdfDocument<'static>, path: Option<PathBuf> }` ; `open` lit le fichier (`std::fs::read`) puis `from_bytes` ; `from_bytes` appelle `pdfium()?.load_pdf_from_byte_vec(bytes, password)`. `title()` via `inner.metadata().get(PdfDocumentMetadataTagType::Title)` → `.value().to_string()` (chaîne vide = `None`). `page_count` = `inner.pages().len()`. `page_size(i)` : vérifier la borne et retourner `PageOutOfRange`, sinon `inner.pages().page_size(i)` → largeur/hauteur du `PdfRect`. `to_bytes` = `save_to_bytes`.
- `render.rs` : `render_page` : borne → `pages().get(i)` → `PdfRenderConfig::new().set_target_width(round(width_pt*scale)).set_target_height(round(height_pt*scale)).render_form_data(true)` → `render_with_config` → `as_image()` → `into_rgba8()`. `to_png` encode avec `image::codecs::png::PngEncoder` dans un `Vec<u8>`.
- `testing.rs` (`#[cfg(feature = "testing")]`) : `sample_pdf` crée `pdfium().create_new_pdf()`, met le titre via `metadata_mut()` si l'API l'offre (sinon le test vérifie `title()` sur un fixture : adapter le test en conséquence et noter la limite), ajoute `pages` pages A4 (`PdfPagePaperSize::a4()`) avec un `PdfPageTextObject` « Page N » (police `PdfFontBuiltin::Helvetica`, 24 pt, position (72, 760)), retourne `save_to_bytes()`.
- `lib.rs` : modules + réexports.

- [ ] **Step 5 : Tests verts, fixture simple**

Run : `cargo test -p mega-core`
Expected : 8 tests OK (dont le test chiffré une fois la fixture créée).
Puis ajouter à `xtask` la sous-commande `gen-fixtures` qui écrit `fixtures/simple.pdf` = `sample_pdf(3, "MegaUltraPDF — exemple")` (xtask dépend de `mega-core` avec feature `testing`). Run : `cargo xtask gen-fixtures` ; commiter `fixtures/simple.pdf` et `fixtures/encrypted.pdf`.

- [ ] **Step 6 : Clippy, docs, commit**

Run : `cargo clippy -p mega-core --all-targets -- -D warnings; cargo doc -p mega-core --no-deps`
Expected : aucun avertissement.
```bash
git add crates/mega-core xtask fixtures
git commit -m "feat(core): open documents, read metadata and render pages with PDFium"
```

---

### Tâche 5 : `mega-cli` — `info` et `render`

**Files:**
- Create: `crates/mega-cli/src/cli.rs`, `crates/mega-cli/src/commands/info.rs`, `crates/mega-cli/src/commands/render.rs`, `crates/mega-cli/src/commands/mod.rs`, `crates/mega-cli/tests/cli.rs`
- Modify: `crates/mega-cli/src/main.rs`, `crates/mega-cli/Cargo.toml`

**Interfaces:**
- Consumes: `mega_core::{Document, CoreError}`.
- Produces: binaire `mega` ; `mega info <fichier> [--password <mdp>]` affiche en français `Fichier : …`, `Titre : …` (ou `(aucun)`), `Pages : N`, `Page 1 : L × H pt` ; code de sortie 1 et message `Erreur : …` sur stderr en cas d'échec. `mega render <fichier> --page <n à partir de 1> --scale <f> --out <png>`.

- [ ] **Step 1 : Tests d'intégration**

`crates/mega-cli/Cargo.toml` : `clap = { version = "4", features = ["derive"] }`, `[dev-dependencies] assert_cmd = "2", predicates = "3", tempfile = "3"`.
`crates/mega-cli/tests/cli.rs` :
```rust
use assert_cmd::Command; use predicates::prelude::*;
fn fixture(name: &str) -> String { format!("{}/../../fixtures/{name}", env!("CARGO_MANIFEST_DIR")) }

#[test] fn info_prints_page_count_and_title() {
    Command::cargo_bin("mega").unwrap().args(["info", &fixture("simple.pdf")]).assert().success()
        .stdout(predicate::str::contains("Pages : 3")).stdout(predicate::str::contains("Titre : MegaUltraPDF"));
}
#[test] fn info_reports_missing_file() {
    Command::cargo_bin("mega").unwrap().args(["info", "introuvable.pdf"]).assert().code(1)
        .stderr(predicate::str::contains("Erreur"));
}
#[test] fn info_handles_unicode_path() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("mes documents résumé.pdf");
    std::fs::copy(fixture("simple.pdf"), &p).unwrap();
    Command::cargo_bin("mega").unwrap().args(["info", p.to_str().unwrap()]).assert().success();
}
#[test] fn info_encrypted_needs_password() {
    Command::cargo_bin("mega").unwrap().args(["info", &fixture("encrypted.pdf")]).assert().code(1)
        .stderr(predicate::str::contains("mot de passe"));
    Command::cargo_bin("mega").unwrap().args(["info", &fixture("encrypted.pdf"), "--password", "secret"]).assert().success();
}
#[test] fn render_writes_png() {
    let dir = tempfile::tempdir().unwrap(); let out = dir.path().join("p1.png");
    Command::cargo_bin("mega").unwrap().args(["render", &fixture("simple.pdf"), "--page", "1", "--scale", "1.5", "--out", out.to_str().unwrap()]).assert().success();
    assert!(std::fs::metadata(&out).unwrap().len() > 100);
}
```

- [ ] **Step 2 : Vérifier l'échec**

Run : `cargo test -p mega-cli`
Expected : échec (`info` inconnu / pas de sous-commande).

- [ ] **Step 3 : Implémenter**

`cli.rs` : `#[derive(Parser)] struct Cli { #[command(subcommand)] command: Command }`, `enum Command { Info { file: PathBuf, #[arg(long)] password: Option<String> }, Render { file: PathBuf, #[arg(long, default_value_t = 1)] page: u16, #[arg(long, default_value_t = 1.0)] scale: f32, #[arg(long)] out: PathBuf } }`.
`commands/info.rs` : `pub fn run(file: &Path, password: Option<&str>) -> anyhow::Result<()>`.
`commands/render.rs` : `pub fn run(file: &Path, page: u16, scale: f32, out: &Path) -> anyhow::Result<()>` (page 1-based → index 0-based ; `std::fs::write(out, rendered.to_png()?)`).
`main.rs` : parse, dispatch ; sur `Err(e)` : `eprintln!("Erreur : {e:#}")` et `std::process::exit(1)`. Le message de `CoreError::PasswordRequired` doit contenir « mot de passe ».

- [ ] **Step 4 : Tests verts**

Run : `cargo test -p mega-cli; cargo run -p mega-cli -- info fixtures/simple.pdf`
Expected : 5 tests OK ; sortie `Pages : 3`.

- [ ] **Step 5 : Commit**

```bash
git add crates/mega-cli
git commit -m "feat(cli): add info and render commands"
```

---

### Tâche 6 : Échafaudage Tauri 2 + Svelte

**Files:**
- Create: `app/` (généré), puis modifier `app/src-tauri/Cargo.toml`, `app/src-tauri/tauri.conf.json`, `app/src-tauri/capabilities/default.json`, `app/package.json`, `app/src/lib/strings.ts`, `app/src/App.svelte`
- Modify: `Cargo.toml` (racine : ajouter `"app/src-tauri"` aux membres), `.gitignore`

**Interfaces:**
- Produces: `pnpm --dir app tauri dev` ouvre une fenêtre « MegaUltraPDF » ; scripts pnpm `dev`, `build`, `check`, `lint`, `format`, `test`, `tauri` ; crate `mega-app` dans le workspace ; plugin dialog enregistré ; CSP autorisant `mega:` et `http://mega.localhost` en `img-src`.

- [ ] **Step 1 : Générer le projet**

Run (racine) : `pnpm create tauri-app app --template svelte-ts` (répondre pnpm si demandé ; identifiant `org.megaultrapdf.app`). Puis `pnpm --dir app install`.
Expected : dossier `app/` avec `src/`, `src-tauri/`, `package.json`.

- [ ] **Step 2 : Intégrer au workspace**

- Racine `Cargo.toml` : membres `["crates/*", "xtask", "app/src-tauri"]`.
- `app/src-tauri/Cargo.toml` : `name = "mega-app"`, `edition.workspace = true`, `license.workspace = true`, `[lints.rust] unsafe_code = "forbid"`, dépendances `mega-core = { path = "../../crates/mega-core" }`, `tauri-plugin-dialog = "2"`, `serde.workspace = true`, `thiserror.workspace = true`. Supprimer le `Cargo.lock` généré dans `app/src-tauri` (un seul lock à la racine).
- `tauri.conf.json` : `productName = "MegaUltraPDF"`, `identifier = "org.megaultrapdf.app"`, fenêtre `title = "MegaUltraPDF"`, 1200×800, `app.security.csp = { "default-src": "'self'", "img-src": "'self' mega: http://mega.localhost blob: data:", "style-src": "'self' 'unsafe-inline'", "connect-src": "ipc: http://ipc.localhost" }`, `bundle.active = true`, `bundle.license = "GPL-3.0-or-later"`.
- Plugin dialog : `pnpm --dir app tauri add dialog` (ajoute la dépendance Rust, le paquet npm et `.plugin(tauri_plugin_dialog::init())` dans `lib.rs`) ; vérifier que `capabilities/default.json` contient `"dialog:allow-open"` (sinon l'ajouter) et `"core:default"`.
- Scripts `package.json` : `"check": "svelte-check --tsconfig ./tsconfig.json"`, `"lint": "eslint ."`, `"format": "prettier --write ."`, `"test": "vitest run"` ; ajouter les devDependencies `vitest`, `eslint`, `@eslint/js`, `typescript-eslint`, `eslint-plugin-svelte`, `prettier`, `prettier-plugin-svelte` (dernières versions stables) avec `eslint.config.js` et `.prettierrc` (`"plugins": ["prettier-plugin-svelte"]`, `"singleQuote": true`, `"printWidth": 100`).
- `app/src/lib/strings.ts` : `export const fr = { appName: 'MegaUltraPDF', openFile: 'Ouvrir un PDF…', noDocument: 'Aucun document ouvert.', pages: (n: number) => \`${n} page${n > 1 ? 's' : ''}\`, untitled: '(sans titre)', errorPrefix: 'Erreur : ' } as const; export const t = fr;`
- `App.svelte` : remplacer le contenu généré par une page vide avec le nom de l'app et un bouton `t.openFile` (sans action pour l'instant), SPDX en commentaire HTML.
- `.gitignore` : vérifier `app/node_modules/`, `app/dist/`, `app/src-tauri/target/` (inutile si le workspace partage `/target`, ce qui est le cas), `app/src-tauri/gen/`.

- [ ] **Step 3 : Vérifier**

Run : `cargo build --workspace; pnpm --dir app check; pnpm --dir app lint; pnpm --dir app tauri dev`
Expected : build OK ; check et lint sans erreur ; fenêtre « MegaUltraPDF » avec le bouton. Fermer la fenêtre.

- [ ] **Step 4 : Commit**

```bash
git add -A
git commit -m "feat(app): scaffold Tauri 2 + Svelte 5 desktop shell"
```

---

### Tâche 7 : Backend Tauri — registre de documents et protocole `mega://`

**Files:**
- Create: `app/src-tauri/src/registry.rs`, `app/src-tauri/src/commands.rs`, `app/src-tauri/src/protocol.rs`, `app/src-tauri/src/error.rs`
- Modify: `app/src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `mega_core::{Document, PageSize, CoreError}`.
- Produces :
  - `#[derive(Serialize)] struct DocumentInfo { id: u32, path: String, title: Option<String>, page_count: u16, pages: Vec<PageSize> }` (reflété en TS en Tâche 8).
  - Commandes Tauri : `open_document(path: String, password: Option<String>) -> Result<DocumentInfo, AppError>`, `close_document(id: u32) -> Result<(), AppError>`.
  - `AppError` (`thiserror`, sérialisé en `{ kind: "password_required" | "invalid_pdf" | "not_found" | "io" | "internal", message: String }` via `impl Serialize`).
  - `struct DocumentRegistry { next_id: u32, docs: HashMap<u32, Document> }` avec `insert(Document) -> u32`, `get(u32) -> Option<&Document>`, `remove(u32) -> bool` ; stocké en `tauri::State<Mutex<DocumentRegistry>>`.
  - Protocole `mega` : chemin **percent-encodé par `convertFileSrc`** côté TS, donc le backend décode `request.uri().path()` (sans le `/` initial) puis attend `page/<id>/<index>` et la query `scale=<f32>` (défaut 1.0, borné à [0.1, 8.0]). Réponse `200 image/png` ; `404` si document ou page inconnus ; `400` si le chemin est mal formé.
  - `pub fn parse_page_request(path: &str, query: Option<&str>) -> Result<PageRequest, ProtocolError>` avec `struct PageRequest { id: u32, index: u16, scale: f32 }`.

- [ ] **Step 1 : Tests unitaires**

Dans `protocol.rs` :
```rust
#[test] fn parse_ok() {
    let r = parse_page_request("page/3/0", Some("scale=2")).unwrap();
    assert_eq!((r.id, r.index, r.scale), (3, 0, 2.0));
}
#[test] fn parse_decodes_percent_encoding() {
    assert_eq!(parse_page_request("page%2F3%2F1", None).unwrap().index, 1);
}
#[test] fn parse_rejects_bad_index() { assert!(parse_page_request("page/3/abc", None).is_err()); }
#[test] fn parse_rejects_unknown_route() { assert!(parse_page_request("thumb/3/0", None).is_err()); }
#[test] fn scale_is_clamped() { assert_eq!(parse_page_request("page/1/0", Some("scale=99")).unwrap().scale, 8.0); }
```
Dans `registry.rs` (avec `mega-core` feature `testing` en dev-dependency) :
```rust
#[test] fn insert_get_remove() {
    let mut r = DocumentRegistry::default();
    let id = r.insert(Document::from_bytes(mega_core::testing::sample_pdf(1, "a"), None).unwrap());
    assert!(r.get(id).is_some()); assert!(r.remove(id)); assert!(r.get(id).is_none());
}
```

- [ ] **Step 2 : Vérifier l'échec**

Run : `cargo test -p mega-app`
Expected : erreur de compilation.

- [ ] **Step 3 : Implémenter**

- `app/src-tauri/Cargo.toml` : ajouter `percent-encoding = "2"`, `http = "1"` (réexporté par tauri sinon : `tauri::http`), `[dev-dependencies] mega-core = { path = "../../crates/mega-core", features = ["testing"] }`.
- `error.rs` : `AppError` + `From<CoreError>` (mapping des `kind` ci-dessus) + `Serialize` manuel.
- `registry.rs` : la struct et ses trois méthodes ; `// [Apprentissage] HashMap et ownership : le registre possède les documents`.
- `commands.rs` : les deux `#[tauri::command]` ; `open_document` construit `DocumentInfo` (pages via `page_sizes()`).
- `protocol.rs` : `parse_page_request` ; `pub fn register(builder: tauri::Builder<R>) -> tauri::Builder<R>` qui appelle `register_uri_scheme_protocol("mega", |ctx, request| …)` : récupère `ctx.app_handle().state::<Mutex<DocumentRegistry>>()`, verrouille, rend, encode PNG, répond avec `Content-Type: image/png` et `Cache-Control: no-store`. Codes 400/404/500 avec corps texte. Sur Windows, Tauri sert ce protocole sous `http://mega.localhost/…` (documenté).
- `lib.rs` : `.manage(Mutex::new(DocumentRegistry::default()))`, `.plugin(tauri_plugin_dialog::init())`, `protocol::register(builder)`, `.invoke_handler(tauri::generate_handler![commands::open_document, commands::close_document])`.

- [ ] **Step 4 : Tests verts et vérification manuelle**

Run : `cargo test -p mega-app; cargo clippy -p mega-app --all-targets -- -D warnings`
Expected : 6 tests OK, clippy propre. Lancer `pnpm --dir app tauri dev`, ouvrir les DevTools (F12), dans la console : `await window.__TAURI__.core.invoke('open_document', { path: 'C:\\\\Users\\\\labre\\\\Desktop\\\\MegaUltraPDF\\\\fixtures\\\\simple.pdf' })` → objet `{ id: 1, page_count: 3, … }` (nécessite `app.withGlobalTauri = true` temporairement dans `tauri.conf.json`, à retirer ensuite). Puis naviguer un onglet du WebView vers `http://mega.localhost/page%2F1%2F0?scale=1` → image PNG visible.

- [ ] **Step 5 : Commit**

```bash
git add app/src-tauri
git commit -m "feat(app): document registry, open/close commands and mega:// page protocol"
```

---

### Tâche 8 : Frontend — ouvrir un PDF et afficher la page 1

**Files:**
- Create: `app/src/lib/api.ts`, `app/src/lib/pageUrl.ts`, `app/src/lib/pageUrl.test.ts`, `app/src/lib/components/PageImage.svelte`
- Modify: `app/src/App.svelte`, `app/src/lib/strings.ts`, `app/vite.config.ts` (config Vitest : `test: { environment: 'node', include: ['src/**/*.test.ts'] }`)

**Interfaces:**
- Consumes: commandes `open_document`/`close_document`, `DocumentInfo`, protocole `mega`.
- Produces :
  - `api.ts` : `export interface PageSize { width_pt: number; height_pt: number }`, `export interface DocumentInfo { id: number; path: string; title: string | null; page_count: number; pages: PageSize[] }`, `export interface AppError { kind: 'password_required' | 'invalid_pdf' | 'not_found' | 'io' | 'internal'; message: string }`, `export async function openDocument(path: string, password?: string): Promise<DocumentInfo>`, `export async function closeDocument(id: number): Promise<void>`, `export async function pickPdfFile(): Promise<string | null>` (dialog `open` avec filtre `{ name: 'PDF', extensions: ['pdf'] }`).
  - `pageUrl.ts` : `export function pagePath(id: number, index: number): string` → `page/${id}/${index}` ; `export function pageUrl(id: number, index: number, scale: number): string` → `convertFileSrc(pagePath(id, index), 'mega') + '?scale=' + scale` (avec `convertFileSrc` de `@tauri-apps/api/core`).

- [ ] **Step 1 : Test Vitest**

`pageUrl.test.ts` :
```ts
import { describe, expect, it, vi } from 'vitest';
vi.mock('@tauri-apps/api/core', () => ({ convertFileSrc: (p: string, s: string) => `http://${s}.localhost/${encodeURIComponent(p)}` }));
import { pagePath, pageUrl } from './pageUrl';
describe('pageUrl', () => {
  it('builds the backend path', () => expect(pagePath(3, 0)).toBe('page/3/0'));
  it('appends the scale query', () => expect(pageUrl(3, 0, 2)).toBe('http://mega.localhost/page%2F3%2F0?scale=2'));
});
```

- [ ] **Step 2 : Vérifier l'échec**

Run : `pnpm --dir app test`
Expected : échec (module `./pageUrl` introuvable).

- [ ] **Step 3 : Implémenter**

- `pageUrl.ts`, `api.ts` comme ci-dessus (`invoke` de `@tauri-apps/api/core`, `open` de `@tauri-apps/plugin-dialog`).
- `strings.ts` : ajouter `openError: (m: string) => \`Impossible d'ouvrir le document : ${m}\``, `passwordRequired: 'Ce document est protégé par un mot de passe.'`, `pageOf: (n: number, total: number) => \`Page ${n} / ${total}\``.
- `PageImage.svelte` : props `{ docId: number; index: number; scale: number; size: PageSize }` ; rend `<img src={pageUrl(docId, index, scale)} alt={t.pageOf(index + 1, …)} width={size.width_pt * scale} height={size.height_pt * scale} />` avec un fond gris clair pendant le chargement.
- `App.svelte` : état `$state` `doc: DocumentInfo | null`, `error: string | null` ; bouton `t.openFile` → `pickPdfFile()` → `openDocument()` ; affiche titre (`t.untitled` si null), `t.pages(doc.page_count)`, et `<PageImage>` de la page 0 à l'échelle 1.5 ; `error` affiché en rouge ; si `AppError.kind === 'password_required'`, afficher `t.passwordRequired` (la saisie du mot de passe arrive au Lot 1). Mise en page minimale : barre d'outils en haut, zone centrale défilante, fond gris. Pas de framework CSS.

- [ ] **Step 4 : Vérifier**

Run : `pnpm --dir app test; pnpm --dir app check; pnpm --dir app lint; pnpm --dir app tauri dev`
Expected : 2 tests OK, check/lint propres ; dans la fenêtre : clic sur « Ouvrir un PDF… », choisir `fixtures/simple.pdf` → « MegaUltraPDF — exemple », « 3 pages », page 1 affichée avec le texte « Page 1 ». Choisir `fixtures/encrypted.pdf` → message mot de passe. Choisir un fichier `.txt` renommé `.pdf` → message d'erreur, l'application reste utilisable.

- [ ] **Step 5 : Commit**

```bash
git add app/src app/vite.config.ts
git commit -m "feat(app): open a PDF from a dialog and display its first page"
```

---

### Tâche 9 : Environnement Claude Code

**Files:**
- Create: `CLAUDE.md`, `.claude/settings.json`, `.claude/hooks/format-on-edit.ps1`, `.claude/hooks/verify-on-stop.ps1`, `.claude/skills/lot/SKILL.md`, `.claude/skills/explique/SKILL.md`, `.claude/skills/concept/SKILL.md`, `.claude/skills/note-apprentissage/SKILL.md`, `.claude/skills/verif/SKILL.md`, `.claude/skills/pdf-spec/SKILL.md`, `.claude/agents/rust-reviewer.md`, `.claude/agents/pdf-expert.md`, `.claude/agents/ui-reviewer.md`, `.vscode/extensions.json`, `.vscode/settings.json`

**Interfaces:**
- Produces: les règles et automatismes décrits au §6 de la spec.

- [ ] **Step 1 : `CLAUDE.md`** (≤ 120 lignes, en français) avec les sections : Projet (une phrase + lien vers la spec et `docs/ROADMAP.md`) ; Architecture et frontières (copie des trois puces du §4) ; Conventions (langues, SPDX, `forbid(unsafe_code)`, pas de chaînes en dur, pas de réseau) ; Pédagogie (`///` en français qui dit pourquoi, `// [Apprentissage]`, résumé « ce que vous venez de voir » à la fin de chaque tâche, niveau : quelques tutoriels Rust) ; Processus (TDD strict Rust + logique TS, composants Svelte via checklist, un lot = `/lot n` → spec → plan → exécution → `/note-apprentissage`, `/verif` avant de déclarer fini) ; Commandes canoniques (liste des Global Constraints) ; Pièges connus (PDFium non `Send` sans `thread_safe`, `mega://` devient `http://mega.localhost` sous Windows, DLL via `cargo xtask fetch-pdfium`).

- [ ] **Step 2 : `settings.json` et hooks**

`.claude/settings.json` :
```json
{
  "permissions": {
    "allow": ["Bash(cargo *)", "Bash(rustup *)", "Bash(pnpm *)", "Bash(git status*)", "Bash(git diff*)", "Bash(git log*)", "Bash(git add *)", "Bash(git commit *)", "Bash(gh pr *)", "Bash(gh run *)"]
  },
  "hooks": {
    "PostToolUse": [{ "matcher": "Edit|Write|MultiEdit", "hooks": [{ "type": "command", "command": "powershell -NoProfile -ExecutionPolicy Bypass -File .claude/hooks/format-on-edit.ps1" }] }],
    "Stop": [{ "matcher": "", "hooks": [{ "type": "command", "command": "powershell -NoProfile -ExecutionPolicy Bypass -File .claude/hooks/verify-on-stop.ps1", "timeout": 600 }] }]
  }
}
```
`format-on-edit.ps1` : lit le JSON sur stdin, extrait `tool_input.file_path` ; `.rs` → `rustfmt --edition 2024 <file>` ; `.ts`/`.svelte`/`.js`/`.json` sous `app/` → `pnpm --dir app exec prettier --write <file>` ; sinon rien. Toujours sortir 0.
`verify-on-stop.ps1` : `git status --porcelain` ; si aucun fichier modifié → sortie 0 silencieuse ; si des `.rs` ont changé → `cargo clippy --workspace --all-targets -- -D warnings` puis `cargo test --workspace` ; si des fichiers sous `app/src` ont changé → `pnpm --dir app check` et `pnpm --dir app test`. En cas d'échec : écrire un résumé sur stderr et **sortir avec le code 2** (Claude reçoit le résumé et doit corriger) ; sinon écrire « ✔ vérifications OK » et sortir 0. Garde anti-boucle : si la variable d'environnement `CLAUDE_STOP_HOOK_ACTIVE` vaut `1` (fournie par Claude Code dans le JSON `stop_hook_active`), ne pas bloquer une seconde fois.

- [ ] **Step 3 : Compétences** (chaque `SKILL.md` : frontmatter `name`, `description` ; corps en français, ≤ 60 lignes)
- `lot` : « Démarre le lot $ARGUMENTS : lire `docs/ROADMAP.md` et la spec ; invoquer `superpowers:brainstorming` uniquement si le périmètre du lot est flou, sinon rédiger `docs/superpowers/specs/<date>-lot-N-<nom>.md` (périmètre, API ajoutées, concepts Rust, checklist de validation manuelle) ; puis `superpowers:writing-plans` ; marquer le lot « en cours » dans ROADMAP ».
- `explique` : « Expliquer `$ARGUMENTS` (fichier ou symbole) à un apprenant Rust ayant suivi quelques tutoriels : rôle, types en jeu (schéma texte), flux des données, pourquoi ces choix, pièges ; terminer par 2 questions de compréhension. Ne rien modifier. »
- `concept` : « Mini-leçon sur le concept Rust `$ARGUMENTS`, illustrée uniquement avec du code de ce dépôt (citer `fichier:ligne`) ; écrire `docs/apprentissage/concepts/<slug>.md` et l'ajouter à l'index `docs/apprentissage/README.md`. »
- `note-apprentissage` : « En fin de lot : relire les commits du lot (`git log`), lister les concepts rencontrés, écrire `docs/apprentissage/lot-N.md` (concepts, où les voir dans le code, exercices de lecture suggérés), cocher le lot dans ROADMAP. »
- `verif` : « Exécuter dans l'ordre : fmt check, clippy, test, doc, pnpm check/test/lint ; afficher un tableau ✔/✘ ; ne jamais déclarer « fini » si une ligne est ✘. »
- `pdf-spec` : « Rappeler ce que dit ISO 32000-2 sur `$ARGUMENTS` (structures, clés de dictionnaire, contraintes de compatibilité avec les autres lecteurs), proposer les fixtures à créer, et les points à vérifier dans `pdfium-render`. Sources : connaissance interne + recherche web si nécessaire. »

- [ ] **Step 4 : Agents** (frontmatter `name`, `description`, `tools: Read, Grep, Glob, Bash`, `model: inherit`)
- `rust-reviewer` : relecture d'un diff Rust : ownership inutilement cloné, erreurs avalées, API publique sans `///`, `unwrap` hors tests, concept nouveau sans `[Apprentissage]`, frontière `mega-core` respectée. Sortie : liste priorisée avec `fichier:ligne`.
- `pdf-expert` : vérifie qu'une fonctionnalité produit un PDF conforme et lisible ailleurs ; propose des fixtures ; signale les limites de PDFium.
- `ui-reviewer` : Svelte/TS : pas de logique métier, chaînes via `strings.ts`, types alignés sur `api.ts`, accessibilité (labels, focus, clavier).

- [ ] **Step 5 : VS Code**

`.vscode/extensions.json` : les six identifiants de la Tâche 1. `.vscode/settings.json` : `"rust-analyzer.check.command": "clippy"`, `"editor.formatOnSave": true`, `"[svelte]": { "editor.defaultFormatter": "svelte.svelte-vscode" }`, `"[typescript]": { "editor.defaultFormatter": "esbenp.prettier-vscode" }`.

- [ ] **Step 6 : Vérifier les hooks**

Dans une session Claude Code du projet : modifier volontairement l'indentation d'un `.rs` avec Edit → le fichier est reformaté (hook PostToolUse). Introduire `let x: i32 = "a";` dans `mega-cli`, terminer le tour → le hook Stop renvoie l'erreur clippy et bloque ; retirer la ligne → « ✔ vérifications OK ». Lancer `/verif` → tableau tout ✔.

- [ ] **Step 7 : Commit**

```bash
git add CLAUDE.md .claude .vscode
git commit -m "chore: add Claude Code environment (rules, hooks, skills, agents)"
```

---

### Tâche 10 : Documentation, CI et dépôt GitHub

**Files:**
- Create: `docs/ROADMAP.md`, `docs/apprentissage/README.md`, `docs/apprentissage/lot-0.md`, `docs/adr/0001-pdfium-engine.md`, `docs/adr/0002-tauri-svelte-ui.md`, `docs/adr/0003-dual-engine-lopdf.md`, `docs/adr/0004-global-pdfium-oncelock.md`, `CONTRIBUTING.md`, `.github/workflows/ci.yml`
- Modify: `README.md`

**Interfaces:**
- Produces: dépôt distant `github.com/<user>/MegaUltraPDF`, CI verte sur `main`.

- [ ] **Step 1 : Documentation**

- `docs/ROADMAP.md` : tableau des lots 0 à 7 (nom, contenu en une ligne, état : ✅ / 🚧 / ⬜), Lot 0 ✅ une fois la CI verte.
- ADR (format : Contexte / Décision / Conséquences, ≤ 40 lignes) : 0001 PDFium vs MuPDF/hayro ; 0002 Tauri 2 + Svelte vs egui/Dioxus (mention de la demande de l'utilisateur) ; 0003 double moteur, `Document` sérialisable en octets, `lopdf` à partir du Lot 5 ; 0004 `Pdfium` global en `OnceLock` + `thread_safe`, acteur prévu au Lot 1.
- `docs/apprentissage/lot-0.md` (via `/note-apprentissage`) : workspace et crates ; modules et `pub` ; `Result`, `?`, `thiserror` vs `anyhow` ; `struct`/`impl` et ownership dans `Document` ; `OnceLock` ; `serde` et le pont Rust ↔ TS ; `Mutex` dans l'état Tauri ; où lire chaque concept (`fichier:ligne`).
- `CONTRIBUTING.md` : prérequis, `cargo xtask fetch-pdfium`, commandes canoniques, convention de commits (Conventional Commits, anglais), licence.
- `README.md` : compléter avec Fonctionnalités (état des lots), Démarrage rapide (`cargo xtask fetch-pdfium`, `pnpm --dir app install`, `pnpm --dir app tauri dev`), CLI (`mega info`, `mega render`), Licence.

- [ ] **Step 2 : CI**

`.github/workflows/ci.yml` : déclencheurs `push` sur `main` et `pull_request`. Job `check` sur une matrice `ubuntu-latest`, `windows-latest`, `macos-latest` :
1. `actions/checkout@v4` ; `dtolnay/rust-toolchain@stable` avec `components: rustfmt, clippy` ; `Swatinem/rust-cache@v2` ; `pnpm/action-setup@v4` (version 10) ; `actions/setup-node@v4` (node 22, cache pnpm, `cache-dependency-path: app/pnpm-lock.yaml`).
2. Linux seulement : `sudo apt-get update && sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf` (prérequis Tauri 2).
3. `cargo xtask fetch-pdfium` ; `cargo fmt --all --check` ; `cargo clippy --workspace --all-targets -- -D warnings` ; `cargo test --workspace` ; `pnpm --dir app install --frozen-lockfile` ; `pnpm --dir app check` ; `pnpm --dir app lint` ; `pnpm --dir app test` ; `pnpm --dir app tauri build --no-bundle` (compile l'app sans créer d'installeur).
Variable `MEGA_PDFIUM_DIR` non nécessaire : `mega-core` trouve `vendor/pdfium/<triple>/` depuis la racine du workspace.

- [ ] **Step 3 : Authentification GitHub (action de l'utilisateur)**

L'utilisateur tape dans la session : `! gh auth login` (navigateur, compte GitHub, protocole HTTPS). Vérifier : `gh auth status` → « Logged in ».

- [ ] **Step 4 : Créer le dépôt et pousser**

Run : `gh repo create MegaUltraPDF --public --source . --remote origin --description "Lecteur et éditeur PDF libre, sans publicité ni télémétrie, en Rust (PDFium + Tauri)" --push`
(Choisir `--private` si l'utilisateur préfère attendre avant de publier ; lui demander au moment de l'exécution.)
Expected : URL du dépôt affichée ; `gh run watch` montre le job `check` vert sur les trois OS. Corriger et pousser jusqu'à ce que la CI soit verte.

- [ ] **Step 5 : Commit final du lot**

```bash
git add -A
git commit -m "docs: roadmap, ADRs, learning note for batch 0, CI workflow"
git push
```
Marquer le Lot 0 ✅ dans `docs/ROADMAP.md` et pousser.

---

## Vérification de fin de lot (checklist manuelle)

- [ ] `cargo xtask fetch-pdfium` sur un clone propre installe la DLL et `cargo test --workspace` passe.
- [ ] `cargo run -p mega-cli -- info fixtures/simple.pdf` affiche `Pages : 3`.
- [ ] `pnpm --dir app tauri dev` : ouvrir `simple.pdf` → page 1 visible ; `encrypted.pdf` → message mot de passe ; fichier non PDF → erreur sans plantage ; chemin avec accents → OK.
- [ ] Hooks : reformatage automatique après Edit ; blocage sur erreur clippy au Stop.
- [ ] `/verif` tout ✔ ; CI verte sur ubuntu/windows/macos.
- [ ] `docs/apprentissage/lot-0.md` relu par l'utilisateur.
