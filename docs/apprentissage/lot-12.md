# Lot 12 — Identité visuelle : ce que vous venez de voir

## Ce que le lot a construit

- L'application a **sa propre icône** (fenêtre, barre des tâches, installateurs) à la place du
  logo par défaut de Tauri.
- La barre d'outils montre des **icônes sans légende** pour Ouvrir, Enregistrer, Fusionner,
  Scinder, Rechercher et Pivoter ; leur nom reste dans l'infobulle et pour les lecteurs d'écran.
- Les menus Fichier et Contenu gagnent une icône devant certaines entrées.
- Le logo s'affiche quand aucun document n'est ouvert et dans Aide ▸ À propos ; la bannière orne
  le README. Version 0.12.0.

## Concepts Rust rencontrés

1. **Ouvrir et enregistrer une image** : `image::open(&path)` rend une `DynamicImage` (format
   quelconque), `.to_rgba8()` la convertit en grille de pixels `[r, g, b, a]`
   (`xtask/src/brand.rs:181` et `:186`, dans `generate`).
2. **Lire un pixel** : `img.get_pixel(x, y)[3]` est l'opacité (canal alpha) ; on compare à un
   seuil pour ignorer le halo du détourage (`bounding_box`, `xtask/src/brand.rs:108`).
3. **`Option` comme accumulateur** : `found` vaut `None` tant qu'aucun pixel n'est vu, puis
   `Some((x0, y0, x1, y1))` qu'on élargit ; `.map(...)` transforme le résultat final en
   `Rect` (`xtask/src/brand.rs:105`). Fiche :
   [enums et match exhaustif](concepts/enums-et-match-exhaustif.md).
4. **`match` sur un couple** : `match (drawn, start)` traite d'un coup « pixel dessiné ? » et
   « une suite est-elle en cours ? » (`runs`, `xtask/src/brand.rs:78`).
5. **Itérateurs** : `bands.into_iter().step_by(2)` garde une bande sur deux, donc les icônes et
   pas les légendes (`xtask/src/brand.rs:151`) ; `ICON_NAMES.iter().zip(icons)` associe chaque
   nom à son image. Fiche : [itérateurs et fermetures](concepts/iterateurs-et-fermetures.md).
6. **`image::imageops`** : `crop_imm` découpe sans modifier l'original, `overlay` colle sur un
   carré transparent, `resize(..., FilterType::Lanczos3)` réduit proprement
   (`xtask/src/brand.rs:161-165`). `DynamicImage::resize` garde les proportions : la hauteur
   donnée (`u32::MAX`) n'est qu'un plafond (`xtask/src/brand.rs:195`).
7. **Conversions sûres** : `u32::try_from(top)?` refuse un nombre trop grand au lieu de le
   tronquer en silence ; `i64::from(dx)` élargit sans perte. Fiche :
   [Result et l'opérateur ?](concepts/result-et-operateur-point-d-interrogation.md).

Côté TypeScript : `ICON_NAMES = [...] as const` puis `type IconName = (typeof ICON_NAMES)[number]`
fabrique un type qui n'accepte que les 12 noms existants (`app/src/lib/logic/icons.ts:19-22`) :
`<Icon name="ouvrir" />` ne compile pas.

## Décisions de conception

- **Originaux versionnés, images dérivées par un programme** (`assets/brand/`, `cargo xtask
  brand`) : refaire les icônes est reproductible, jamais une retouche à la main.
- **Découpage par bandes, pas par tiers fixes** : les icônes de la dernière rangée débordent
  au-dessus du tiers exact de la planche ; un découpage fixe les aurait rognées.
- **Icônes décoratives** (`alt=""`, `aria-hidden`) : le nom vient du bouton (`aria-label`,
  ADR-0012), donc rien ne change pour un lecteur d'écran.
- **Bannière en JPEG** : image « photo », dix fois plus légère qu'en PNG.
- L'icône de l'application est produite par l'outil officiel `tauri icon` (tous les formats :
  ico, icns, png).

## Pièges rencontrés

- **Un scénario CDP changeait le thème sans effet** : le sélecteur affichait « Sombre », mais
  l'interface restait claire. Svelte 5 **délègue** l'événement `change` à la racine du
  document ; un `new Event('change')` ne remonte pas (`bubbles` vaut `false` par défaut) et
  n'atteint jamais le gestionnaire. Il faut `new Event('change', { bubbles: true })`. Le
  scénario vérifie maintenant `data-theme`.
- **Un test qui ne pouvait pas échouer** (trouvé à la relecture) : le premier test sur la vraie
  planche vérifiait que le bas de chaque icône était transparent. Or la marge ajoutée par
  `square_canvas` l'est toujours, légende restée ou non. Il a été remplacé par une planche
  synthétique (une couleur par case, légendes bleues), et on a vérifié qu'il échoue quand on
  garde les légendes ou qu'on inverse l'ordre des boucles. Leçon : un test se juge à ce qu'il
  **ferait échouer**.
- Le libellé du menu est « Reconnaître le texte (**OCR**) » ; l'icône de la planche anglaise,
  la seule transparente, porte « OCR » : elle colle au libellé.

## Exercices de lecture

1. `xtask/src/brand.rs`, `runs` : pourquoi faut-il **recoller** deux suites séparées par un
   petit trou avant de compter les bandes ?
2. `xtask/src/brand.rs`, test `cut_icons_keeps_the_sheet_order_and_drops_the_captions` :
   pourquoi donner à chaque case **sa propre couleur** d'icône, et à toutes les légendes la
   **même** couleur ? Quelles erreurs du code ce test attraperait-il ?
3. `app/src/lib/components/Icon.svelte` : pourquoi l'image a-t-elle `alt=""` alors que le
   bouton, lui, a un nom ?

## Prérequis pour la suite

- Savoir lire une boucle sur des pixels et un `Option` qu'on remplit au fil de l'eau.
- Distinguer un original (versionné) d'un fichier produit par un outil.
