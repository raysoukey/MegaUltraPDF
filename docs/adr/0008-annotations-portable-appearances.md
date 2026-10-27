# ADR-0008 — Annotations : sous-types choisis, apparences portables, limites de pdfium-render

Date : 2026-10-26 · Statut : accepté (Lot 3)

## Contexte

Le Lot 3 crée des annotations (marquages de texte, notes, texte libre, encre, formes,
tampons) qui doivent être **lisibles à l'identique** dans les autres lecteurs et
annulables (ADR-0007). Deux contraintes du projet pèsent sur la conception :
`#![forbid(unsafe_code)]` dans `mega-core` (donc pas d'appel aux bindings bruts de
pdfium-render), et PDFium comme unique moteur de rendu (donc ce que nous écrivons doit
aussi s'afficher chez nous).

Ce que pdfium-render 0.9.4 permet sans `unsafe` : créer `Text`, `Highlight`, `Underline`,
`StrikeOut`, `Squiggly`, `Square`, `Ink`, `Stamp`, `FreeText`, `Popup`, `Link` ; poser
`/Rect`, `/Contents`, `/T`, dates, `/C` (+ `/CA` via l'alpha), `/IC`, drapeaux, `/QuadPoints` ;
ajouter des objets chemin et texte à l'apparence d'une **`Ink` ou d'un `Stamp`** uniquement
(`FPDFAnnot_AppendObject`). Non exposé : `Circle`, `Line`, `/InkList`, `/BS` (épaisseur),
`FPDFAnnot_SetAP`. PDFium ne génère une apparence par défaut que pour `Square`, `Circle`,
`Highlight`, `Ink`, `Popup`, `Squiggly`, `StrikeOut`, `Underline` — pas pour `Text`,
`FreeText`, `Line`, `Stamp`.

## Décision

| Genre (interface) | Sous-type PDF | Apparence | Pourquoi |
|---|---|---|---|
| Surligner, souligner, barrer, ondulé | `Highlight`, `Underline`, `StrikeOut`, `Squiggly` + `/QuadPoints` + `/C` | générée par PDFium au premier rendu ; les autres lecteurs la génèrent aussi | sous-types natifs, sémantique exacte |
| Note | `Text` + `/Contents`, `/T`, dates, `/C` | générée par PDFium au premier rendu (bulle remplie de la couleur, cernée de noir) ; les autres lecteurs dessinent leur icône | sous-type natif ; le calque SVG ne porte que la zone cliquable |
| Rectangle, ellipse, ligne, flèche, encre | `Ink` + objets chemin (couleur, épaisseur, opacité, remplissage) | explicite (`/AP`), identique partout | seul sous-type « forme » qui accepte une apparence ; `Circle`/`Line`/`/BS` inaccessibles |
| Texte libre | `Stamp` + objets texte Helvetica + `/Contents` | explicite | `FreeText` serait invisible dans MegaUltraPDF (pas de génération, pas d'objets) |
| Tampon prédéfini | `Stamp` + cadre + texte centré + `/Contents` | explicite | — |

- **La couleur avant l'apparence** : `FPDFAnnot_SetColor` refuse une annotation qui a un
  `/AP`. Ordre imposé dans `add_annotation` : `/Rect`, couleurs, métadonnées, puis objets.
- **Modifier = supprimer + recréer** (`UpdateAnnotation`), **déplacer une forme = recréer**
  avec la géométrie remise à l'échelle entre les deux emprises intérieures (l'apparence est
  en coordonnées de page ; changer `/Rect` seul la laisse sur place ou la déforme selon
  les lecteurs). Une note (sans `/AP`) change simplement de `/Rect`. Les marquages ne se
  déplacent pas. L'index d'une annotation recréée change (fin de tableau) : l'interface
  resynchronise sa sélection après chaque édition.
- **Relecture par l'apparence** : le genre d'une `Ink`/`Stamp` et sa géométrie sont
  reconstruits en analysant les segments de ses objets chemin (2 segments → ligne ;
  Bézier seulement → ellipse ; 3-4 segments axiaux fermés → rectangle ; ligne + chemin à
  3 segments → flèche ; sinon encre) et ses objets texte (texte seul → texte libre).
  Une annotation lue est ainsi toujours recréable (`Annotation::to_new`).
- **`/C` jamais lu directement** : `stroke_color()` / `fill_color()` de pdfium-render 0.9.4
  sur une annotation qui possède un `/AP` transtypent le handle en objet de page
  (comportement indéfini : plantage ou noir renvoyé au hasard), et PDFium génère un `/AP`
  pour presque tout dès le premier rendu. La couleur vient donc **uniquement** des objets
  d'apparence (remplissage pour une note, un surlignage ou une forme tierce ; trait pour nos
  formes) ; elle est inconnue (`None`) avant le premier rendu. Bornes de lecture : 2000
  annotations par page, 512 quadrilatères, 4096 caractères par chaîne.
- Rendu avec `FPDF_ANNOT` (`render_annotations(true)`) : les annotations existantes
  apparaissent dans l'image de la page ; le calque SVG ne porte que l'interaction (icônes
  de note, poignées, aperçu du tracé).

## Alternatives écartées

- `Square`/`Circle`/`Line` natifs avec `/BS` : impossible sans bindings bruts ; à revoir
  avec `lopdf` (ADR-0003, Lot 5) qui pourra écrire les vrais sous-types et `/InkList`.
- `FreeText` + `/DA` : conforme mais invisible dans PDFium ; un lecteur qui régénère
  l'apparence (Acrobat) l'afficherait, pas nous.
- Déplacer par `FPDFAnnot_SetRect` seul : comportement différent selon que le nouveau
  rectangle contient l'ancien (PDFium réécrit alors `/BBox`) ou non (mise à l'échelle).

## Conséquences

- Un rectangle de MegaUltraPDF est une annotation `Ink` pour Acrobat : il s'affiche à
  l'identique, mais Acrobat le présentera comme un dessin, pas comme un rectangle éditable.
  Idem pour les textes libres (tampons). Documenté dans la note du lot.
- Pas d'`/InkList` : un outil qui régénère l'apparence depuis `/InkList` perdrait le tracé
  (rare, à l'édition seulement).
- Tests : `crates/mega-core/tests/annotations.rs` (lecture tierce, aller-retour par genre,
  déplacement, validation), `tests/edit.rs`, `tests/service.rs`, CLI `annots`, scénarios
  CDP `annotation.json` et `annotation-tierce.json`.
