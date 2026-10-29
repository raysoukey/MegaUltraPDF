# Lot 12 — Identité visuelle : plan d'implémentation

> Exécution native (executing-plans), registre `.superpowers/sdd/2026-10-29-lot-12-identite-visuelle/progress.md`.

**But :** intégrer les visuels du projet : icône d'application, 12 icônes d'actions sans légende, logo, bannière du README.
**Spec :** `docs/superpowers/specs/2026-10-29-lot-12-identite-visuelle.md`.

## Contraintes globales
- Aucune chaîne visible nouvelle hors `strings.ts` ; boutons à icône seule : `aria-label` + `title` (ADR-0012).
- Images dérivées produites par `cargo xtask brand` et `tauri icon`, jamais retouchées à la main.
- Aucun appel réseau : images servies depuis `app/static/`.

## Points d'attention (non couverts par les tests unitaires)
1. Une légende résiduelle dans une icône (découpage) : contrôle visuel des 12 PNG.
2. Icônes illisibles en thème sombre : captures CDP claires et sombres.
3. Bouton devenu muet pour un lecteur d'écran : scénario CDP (aria-label sur chaque bouton à icône).
4. Icône de l'exécutable installé encore celle de Tauri : extraction de l'icône du paquet NSIS.
5. Scénarios existants qui ciblaient un libellé de bouton : relancer les 24.

### Tâche 1 : découpage reproductible (`xtask/src/brand.rs`)
- Tests : `runs_separate_the_icon_from_its_caption`, `runs_bridge_small_holes_and_drop_specks`, `bounding_box_finds_drawn_pixels_in_the_zone`, `square_canvas_centres_the_drawing`, `the_project_sheet_gives_twelve_captionless_icons`.
- Produit : `runs(&[bool], min_gap, min_len) -> Vec<(usize, usize)>`, `bounding_box(&RgbaImage, Range<u32>, Range<u32>) -> Option<Rect>`, `square_canvas(w, h, margin) -> (side, dx, dy)`, `cut_icons(&RgbaImage) -> Result<Vec<RgbaImage>>`, `generate(root)`, `ICON_NAMES`.
- Puis `cargo xtask brand`, `tauri icon`, commit des sources et des images dérivées.

### Tâche 2 : icônes dans l'interface
- `logic/icons.ts` : `IconName`, `ICON_NAMES` (même ordre que xtask), `iconSrc(name)` ; test `icons.test.ts` (fichier présent, PNG 64 × 64 pour chaque nom).
- `components/Icon.svelte` (image décorative, 24 px, grisée si le bouton est désactivé).
- `Toolbar.svelte` (open, save, merge, split, search, rotate), `FileMenu.svelte` (open, export, protect, sign, compress), `ContentToolbar.svelte` (ocr), `AnnotationToolbar.svelte` (annotate, sign), logo dans `+page.svelte` et `HelpDialog.svelte`.
- Scénario CDP `scripts/scenarios/icones.json`.

### Tâche 3 : documentation et version
- README (bannière, tableau des lots 9 à 12, provenance des visuels), ROADMAP, FONCTIONNALITES, CLAUDE.md, version 0.12.0.
