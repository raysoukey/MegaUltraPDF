# Lot 12 — Identité visuelle : intégrer les images du dossier `images/`

## Contexte

L'utilisateur a créé avec ChatGPT, dans `images/` (non versionné), une identité visuelle :
- un logo complet ;
- une icône carrée ;
- une planche de 12 icônes d'actions ;
- deux bannières, en français et en anglais.

Aujourd'hui, l'application porte encore **le logo par défaut de Tauri** : `app/src-tauri/icons/*` (fenêtre, barre des tâches, installateurs). Sa barre d'outils mêle des mots (« Ouvrir… », « Fusionner… », « Scinder… ») et des émojis (💾 🔍 ⟳).

**Demande** (« intégrer les images, pas de texte sur les icônes »), précisée par les réponses de l'utilisateur :
- icônes **sans leurs légendes** : le badge « PDF » dessiné reste ; les boutons n'affichent plus de mot, le nom passe en infobulle et en nom accessible ;
- intégrer à **quatre endroits** :
  - l'icône de l'application ;
  - les icônes de la barre d'outils et des menus ;
  - le logo sur l'écran d'accueil et dans « À propos » ;
  - la bannière française dans le README.

Résultat visé : une application reconnaissable (icône propre partout) et une barre d'outils illustrée et plus compacte. Release **v0.12.0** à la fin.

## Processus (inchangé)

- `/lot 12`, puis spec `docs/superpowers/specs/2026-10-29-lot-12-identite-visuelle.md` (copie de ce plan) et plan `docs/superpowers/plans/…`.
- Branche `lot-12/identite-visuelle`, registre dans `.superpowers/sdd/`.
- TDD pour les fonctions pures (xtask en Rust, logique TS en Vitest).
- Scénario CDP, `/verif` 8/8.
- Relecture Opus, puis une passe de correction test d'abord.
- `/note-apprentissage`, PR, CI sur 3 OS, fusion, release v0.12.0.
- **Contrôle du paquet installé** avant publication (leçon du Lot 11).

## A. Sources versionnées et découpage reproductible (`cargo xtask brand`)

- Copier les originaux utiles dans `assets/brand/`, sous des noms ASCII :
  - `logo.png` (image 1, transparente) ;
  - `icon.png` (image 2, carrée, transparente) ;
  - `actions.png` (planche des icônes, version **transparente**, 20_53_23-3 ; les légendes sont retirées de toute façon) ;
  - `banner-fr.png` (image 4 FR).
- `assets/brand/README.md` : origine (créées par le propriétaire du projet avec ChatGPT, le 2026-10-26) et correspondance avec les fichiers d'`images/`. Le dossier `images/` reste non versionné.
- Nouvelle sous-commande **`cargo xtask brand`** (`xtask/src/brand.rs`). Elle utilise le crate `image` 0.25 (déjà dans l'arbre via `mega-core`, feature `png`), justifié dans le commit.
  - Découpe la planche en grille fixe de 4 × 3 cases, dans l'ordre : open, save, export, merge, split, rotate, annotate, sign, ocr, search, protect, compress.
  - Dans chaque case : garder la première bande verticale de pixels opaques, qui s'arrête au premier intervalle transparent ; c'est ce qui **retire la légende** placée dessous. Ensuite : cadre englobant, mise au carré avec marge transparente, réduction Lanczos3 en 64 × 64. Résultat : `app/static/icons/<nom>.png`.
  - Logo : réduction à 480 px de large, dans `app/static/logo.png`.
  - Bannière : réduction à 1280 px de large, dans `docs/images/banniere.png`.
  - Fonctions pures testées, d'abord RED puis GREEN :
    - `content_rows(rows: &[bool], min_gap: usize) -> Option<(usize, usize)>` : bande de l'icône, légende exclue ;
    - `bounding_box(img, rows) -> Option<Rect>` ;
    - `square(rect, margin) -> Rect`.
- **Icône de l'application** : `pnpm --dir app tauri icon ../assets/brand/icon.png`. L'outil officiel régénère tout `app/src-tauri/icons/` (ico, icns, png, logos Store) ; `tauri.conf.json` reste inchangé.

## B. Icônes dans l'interface

- `app/src/lib/logic/icons.ts` :
  - type `IconName` (union des 12 noms) ;
  - `ICON_NAMES` ;
  - `iconSrc(name) = '/icons/<nom>.png'`.

  Test Vitest `icons.test.ts` : chaque nom a son fichier dans `app/static/icons/`, en PNG carré 64 × 64 (en-tête lu par `fs`). Le test casse si un fichier manque ou si `xtask brand` change de format.
- Composant `app/src/lib/components/Icon.svelte`. Il affiche `<img src={iconSrc(name)} alt="" aria-hidden="true" draggable="false">`, en 20 px, purement décoratif : le nom vient du bouton (ADR-0012 : `aria-label` sur les boutons à glyphe). En état désactivé : `opacity` et `grayscale`.
- **Barre d'outils** (`Toolbar.svelte`) : icône seule, `aria-label` et `title` gardés (avec raccourci).

  | Bouton actuel | Devient |
  |---|---|
  | « Ouvrir… » | `open` |
  | 💾 Enregistrer | `save` |
  | « Fusionner… » | `merge` |
  | « Scinder… » | `split` |
  | 🔍 Rechercher | `search` |
  | ⟳ Rotation d'affichage | `rotate` |

  - « Enregistrer sous » reste en texte : la planche n'a pas d'icône pour lui.
  - Les glyphes sans équivalent restent tels quels : zoom −/+, ajuster, pages, annuler/rétablir, ☰, ⓘ, ?.
- **Menus** : une icône devant le libellé, qui reste écrit (un menu est du texte).
  - `FileMenu.svelte` : Ouvrir (`open`), Exporter (`export`), Protéger (`protect`), Signer numériquement (`sign`), Optimiser (`compress`). Les entrées gagnent un champ facultatif `icon?: IconName`.
  - `ContentToolbar.svelte` : ROC (`ocr`).
- **Barre d'annotation** (`AnnotationToolbar.svelte`) :
  - ✎ « Modifier l'annotation » devient `annotate` ;
  - l'outil ✍ « Signature » devient `sign`.
- **Logo** :
  - écran sans document (`+page.svelte`, bloc `.empty`) : `logo.png` au-dessus de `t.noDocument`, `alt=""` (le nom est déjà écrit à côté) ;
  - Aide ▸ À propos (`HelpDialog.svelte`) : logo au-dessus de `t.appName`.

  Taille limitée en CSS ; lisible en thème clair et sombre (contour blanc « autocollant » des images).
- Aucune nouvelle chaîne visible : les noms existent déjà dans `strings.ts`, fr et en.

## C. README et documentation

- `README.md` :
  - bannière `docs/images/banniere.png` en tête ;
  - tableau d'état complété des Lots 9 à 12 (il s'arrête au Lot 8) ;
  - ligne « Visuels : créés pour le projet par son propriétaire ».
- ROADMAP (ligne 12), FONCTIONNALITES (ligne [12] « identité visuelle : icône d'application, icônes d'actions, logo »), CLAUDE.md : un piège, « icônes = `cargo xtask brand` puis `tauri icon`, jamais retoucher `app/static/icons` à la main ». Note `docs/apprentissage/lot-12.md`, concept : traitement d'image en Rust (`image`, `RgbaImage`, pixels, `crop_imm`, `resize`).
- Version 0.12.0 (`Cargo.toml`, `app/package.json`, `tauri.conf.json`).

## Fichiers critiques

- Nouveaux :
  - `assets/brand/*` ;
  - `xtask/src/brand.rs` (et l'entrée dans `xtask/src/main.rs`, `xtask/Cargo.toml`) ;
  - `app/static/icons/*.png`, `app/static/logo.png`, `docs/images/banniere.png` ;
  - `app/src/lib/logic/icons.ts` (+ test), `app/src/lib/components/Icon.svelte`.
- Régénérés : `app/src-tauri/icons/*`.
- Modifiés :
  - `Toolbar.svelte`, `FileMenu.svelte`, `ContentToolbar.svelte`, `AnnotationToolbar.svelte`, `HelpDialog.svelte`, `routes/+page.svelte` ;
  - README, docs.

## Vérification

- `cargo test -p xtask` : les fonctions de découpage, avec des images synthétiques (icône, intervalle, légende).
- Puis `cargo xtask brand`, et **contrôle visuel des 12 PNG produits** (lus un à un) : aucune légende, rien de rogné.
- `/verif` 8/8, dont `icons.test.ts` et le test sans réseau (`no-network.test.ts` : chemins locaux seulement).
- Scénario CDP `icones.json` :
  - chaque bouton à icône a une image chargée (`naturalWidth > 0`), un `aria-label` et pas de texte ;
  - le logo est affiché sans document ;
  - les icônes des menus sont présentes ;
  - captures en thème clair et en thème sombre, regardées.
- Les 24 scénarios existants, sans régression (`finition.json` vise l'entrée de menu « Ouvrir » par son texte, qui reste).
- Paquet NSIS construit localement (`pnpm --dir app release`) :
  - installation silencieuse ;
  - icône vérifiée sur `mega-app.exe` (extraction de l'icône par PowerShell `System.Drawing.Icon.ExtractAssociatedIcon`, regardée) ;
  - scénario `icones.json` sur l'application installée ;
  - désinstallation.
- Après la release, même contrôle sur le paquet de la CI, et `.desktop` / icône présents dans le `.deb`.
