# MegaUltraPDF — règles du projet pour Claude Code

Lecteur et éditeur PDF libre (GPL-3.0-or-later), sans publicité ni télémétrie, en Rust.
Double finalité : outil utilisable au quotidien **et** support d'apprentissage de Rust pour
le propriétaire du projet (niveau : quelques tutoriels suivis). Claude écrit tout le code ;
l'utilisateur apprend en le lisant.

- Conception de référence : `docs/superpowers/specs/2026-10-26-megaultrapdf-design.md`
- Feuille de route par lots : `docs/ROADMAP.md`
- Notes d'apprentissage : `docs/apprentissage/`
- Décisions d'architecture : `docs/adr/`

## Architecture et frontières

- `crates/mega-core` : **toute** la logique PDF (PDFium via `pdfium-render`). Seul crate
  autorisé à importer `pdfium_render`.
- `crates/mega-cli` : binaire `mega`, banc d'essai et outil réel. N'appelle que `mega-core`.
- `app/src-tauri` (crate `mega-app`) : fine couche Tauri : commandes `invoke`, protocole
  `mega://` servant des PNG, registre des documents. **Aucune logique métier.**
- `app/src` : frontend SvelteKit (SPA, Svelte 5, TypeScript strict). Présentation seulement ;
  toute décision sur un document est une commande vers Rust. Types partagés : `app/src/lib/api.ts`
  reflète `commands.rs` / `error.rs`.
- `xtask` : tâches de dev (`cargo xtask fetch-pdfium`, `cargo xtask gen-fixtures`).
- Une seule instance PDFium globale (`OnceLock`, feature `thread_safe`) ; patron acteur prévu
  au Lot 1. `Document` possède ses octets (`from_bytes` / `to_bytes`) ; `lopdf` n'arrive qu'au Lot 5.

## Conventions

- Identifiants (Rust, TS) et messages de commit en **anglais** (Conventional Commits) ;
  commentaires, docs `///`, notes, interface en **français**.
- Première ligne de chaque `.rs`, `.ts`, `.js` : `// SPDX-License-Identifier: GPL-3.0-or-later`
  (`<!-- … -->` dans un `.svelte`). `#![forbid(unsafe_code)]` dans chaque crate.
- Aucune chaîne visible en dur dans un composant : tout passe par `app/src/lib/strings.ts`.
- Aucun appel réseau à l'exécution. Seul `cargo xtask fetch-pdfium` télécharge (en dev).
- Pas de nouvelle dépendance sans raison écrite dans le message de commit.
- Données venant d'un PDF = données non fiables (voir `display_safe` dans `mega-cli`).

## Pédagogie (non négociable)

- Tout élément `pub` porte une doc `///` en français qui dit **pourquoi** il existe.
- Un concept Rust nouveau pour l'apprenant est signalé par `// [Apprentissage] …`
  (dans une doc `///` ou `//!`, écrire `\[Apprentissage\]` pour que rustdoc ne le prenne pas
  pour un lien) et renvoie si possible à `docs/apprentissage/`.
- Chaque tâche se termine par un court résumé « **Ce que vous venez de voir** » : les concepts
  mobilisés et où les lire (`fichier:ligne`).
- Préférer le code simple et explicite aux abstractions malines. Pas de macro maison.

## Processus

- **TDD strict** pour `mega-core`, `mega-cli`, `app/src-tauri` et la logique TS pure (Vitest) :
  test écrit, vu échouer, puis implémentation. Les composants Svelte sont validés par la
  checklist manuelle de fin de lot (ou par pilotage CDP, voir `docs/apprentissage/lot-0.md`).
- Un lot = `/lot N` → spec du lot → plan → exécution → `/note-apprentissage` → `/verif`.
- Ne jamais déclarer « fini » sans `/verif` tout vert.
- Compétences : `/lot`, `/explique`, `/concept`, `/note-apprentissage`, `/verif`, `/pdf-spec`.
  Agents : `rust-reviewer`, `pdf-expert`, `ui-reviewer`.

## Commandes canoniques

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc -p mega-core --no-deps
pnpm --dir app check      # svelte-check + tsc
pnpm --dir app lint       # eslint
pnpm --dir app test       # vitest
pnpm --dir app format     # prettier
pnpm --dir app tauri dev -- -- fixtures/simple.pdf   # lance l'app sur un fichier
cargo run -p mega-cli -- info fixtures/simple.pdf
```

## Pièges connus

- PDFium n'est pas thread-safe : ne jamais partager un `PdfDocument` entre threads sans la
  feature `thread_safe` ; ne pas réintroduire `Arc<Mutex<Document>>` hors de l'état Tauri.
- `thread_safe` verrouille **chaque appel**, pas une séquence : l'initialisation passe par
  `OnceLock::get_or_init` et tout chargement de document par `engine::load_bytes` (verrou),
  car `FPDF_GetLastError` est une variable globale. Deux bugs réels de la CI du Lot 0.
- Sous Windows, le protocole `mega://` est vu par le WebView comme `http://mega.localhost/…` ;
  la CSP (`tauri.conf.json`) autorise les deux formes. `convertFileSrc` percent-encode le chemin.
- La DLL PDFium vient de `cargo xtask fetch-pdfium` (`vendor/pdfium/<triple>/`, non commitée).
  Sans elle : `CoreError::LibraryNotFound`.
- `pdfium-render` 0.9 : `PdfPageIndex` est `i32`, pas de setter de métadonnées
  (voir `mega-core/src/testing.rs` pour le squelette PDF écrit à la main).
- Les métadonnées d'un PDF sont du texte PDFDocEncoding ou UTF-16BE+BOM, jamais de l'UTF-8 brut.
