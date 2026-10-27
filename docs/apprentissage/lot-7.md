# Lot 7 — Finition et distribution : note d'apprentissage

## Ce que le lot a construit

L'application parle français **et** anglais (préférence « système / français / anglais »,
changement à chaud sans fermer les documents), se pilote entièrement au clavier (menus
« Fichier » et « Contenu », piège de focus dans tous les dialogues, contour de focus
visible, noms accessibles sur les boutons à glyphe, contrastes vérifiés par un test), se
règle dans un vrai dialogue de préférences (Ctrl+,) et s'explique (F1 : prise en main,
raccourcis, « À propos » avec version et licences tierces). Elle s'installe : installeurs
NSIS et MSI, zip portable, deb, AppImage, dmg, construits par `release.yml` avec PDFium
embarqué. Version 0.7.0. Décisions : ADR-0012.

## Concepts Rust rencontrés

### 1. Profils Cargo et compilation conditionnelle

`cfg!(debug_assertions)` (pas de chemin de la machine de compilation dans un binaire
distribué), `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` (pas de
console), `cfg!(windows)` pour lancer `pnpm` (`xtask/src/licenses.rs:164`).

- Fiche : [`profils-cargo-et-distribution`](concepts/profils-cargo-et-distribution.md).

### 2. `OnceLock` pour un réglage écrit une fois (`crates/mega-core/src/engine.rs:20`)

`APP_LIBRARY_DIR: OnceLock<PathBuf>` : l'application dit au cœur où est PDFium, une seule
fois, avant le premier chargement. Même outil que l'instance PDFium globale, et alternative
sûre à `std::env::set_var` (devenu `unsafe` en Rust 2024). `set_library_dir` renvoie `false`
si c'est trop tard ou déjà fait : l'appelant sait que son réglage n'a pas été retenu.

- Fiche : [`oncelock`](concepts/oncelock.md).

### 3. Lire un JSON partiel avec `serde_json` (`xtask/src/licenses.rs:45`)

`cargo metadata` produit des dizaines de champs par paquet ; la structure `Package` n'en
déclare que quatre, et `serde` ignore les autres. `#[derive(Deserialize)]` sur une structure
privée au module suffit : pas besoin de modéliser tout le format.

### 4. Repli d'encodage explicite (`xtask/src/pdfium.rs:189`)

`decode_notice` essaie UTF-8 (`std::str::from_utf8`), sinon traite chaque octet comme un
caractère Latin-1 (`char::from(u8)`). Cas réel : `licenses/freetype.txt` de l'archive
PDFium. Le test le fixe avec deux octets (`0x52 0xE9` = « Ré »).

### 5. Calculer avant de démarrer (`app/src-tauri/src/lib.rs:48-55`)

`tauri::generate_context!()` est appelé **avant** le `Builder` pour connaître le dossier des
ressources sans attendre `setup` : l'acteur PDFium démarre dans `manage(...)`, plus tôt que
`setup`. `if let … && let …` (chaînes de `let`, Rust 2024) enchaîne deux conditions qui
peuvent chacune échouer sans imbriquer deux `if`.

## Côté interface

- **Deux dictionnaires de même forme** (`strings.ts`) : `en: Strings` avec
  `type Strings = typeof fr` ; une clé oubliée casse `pnpm check`. `t` est rempli par
  `setLanguage` et `{#key ui.language}` (`+page.svelte:367`) remonte l'interface.
- **La décision en logique pure, le DOM dans une action** : `logic/menu.ts`,
  `logic/focus.ts`, `logic/contrast.ts` sont testés sous Vitest ; `actions/trapFocus.ts` et
  `actions/menuNav.ts` les relient au DOM. Une action Svelte (`use:trapFocus`) reçoit
  l'élément monté et rend `{ destroy }`.
- `e.stopPropagation()` dans `menuNav.ts:42` : Échap ferme le menu **sans** atteindre le
  gestionnaire global qui fermerait le dialogue ou la recherche en dessous.
- Le piège de focus a révélé un bogue ancien : une touche simple (R) dans l'aide ou les
  propriétés pivotait le document derrière. La garde modale couvre désormais ces dialogues.

## Vérifier un paquet

`pnpm --dir app release --bundles nsis`, installation silencieuse
(`setup.exe /S /D=<dossier>`), `scripts/ui-check.ps1 -Exe <dossier>\mega-app.exe …`,
désinstallation (`uninstall.exe /S`). PDFium n'était que dans `pdfium/` et
`MEGA_PDFIUM_DIR` vide : le rendu prouve que la recherche dans les ressources fonctionne.

## Exercices (plus tard, à votre rythme)

1. `strings.ts` : retirez une clé du dictionnaire `en` et lancez `pnpm --dir app check`.
   Lisez le message : quelle ligne pointe-t-il, et pourquoi le test de forme échoue-t-il aussi ?
2. `logic/focus.ts` : que renvoie `nextFocus(-1, [false, true, true], true)` ? Tracez la
   boucle à la main avant de l'exécuter dans un test.
3. `engine.rs` : pourquoi `set_library_dir` vérifie-t-il `PDFIUM.get().is_some()` **avant**
   `APP_LIBRARY_DIR.set` ? Que se passerait-il dans l'ordre inverse ?
4. `tauri.release.conf.json` : déplacez (sur le papier) `bundle.resources` dans
   `tauri.conf.json` et prédisez ce que fait `cargo test --workspace` sur un dépôt cloné.

## Ce qui reste ouvert

Paquets signés, mises à jour (anti-objectif : pas de réseau), autres langues, traduction
des erreurs détaillées du cœur, dmg Intel, paquets rpm / Flatpak.

## Prérequis pour le Lot 8 (OCR, signature numérique)

- Processus externes et sorties textuelles (OCR via la commande `tesseract`) : `Command`,
  déjà vu dans `xtask`.
- Features Cargo optionnelles (`ocr`, `signature`) : `features-cargo-et-dependances`.
- Cryptographie : PKCS#12, CMS, `/ByteRange` d'une signature PDF (`/pdf-spec signatures`).
