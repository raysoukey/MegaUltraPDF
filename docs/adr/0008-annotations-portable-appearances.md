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
| Surligner, souligner, barrer, ondulé | `Highlight`, `Underline`, `StrikeOut`, `Squiggly` + `/QuadPoints` (ordre Acrobat : haut-gauche, haut-droit, bas-gauche, bas-droit) + `/C` | générée par PDFium au premier rendu **et enregistrée** ; les autres lecteurs utilisent cet `/AP` | sous-types natifs, sémantique exacte |
| Note | `Text` + `/Contents`, `/T`, dates, `/C`, `/F 28` (Print, NoZoom, NoRotate) | générée par PDFium au premier rendu et enregistrée : bulle **jaune fixe** (PDFium ignore `/C`), contour noir ; les autres lecteurs affichent cet `/AP` plutôt que leur icône | sous-type natif ; le calque SVG ne porte que la zone cliquable |
| Rectangle, ellipse, ligne, flèche, encre | `Ink` + objets chemin (couleur, épaisseur, opacité, remplissage) | explicite (`/AP`), identique partout | seul sous-type « forme » qui accepte une apparence ; `Circle`/`Line`/`/BS` inaccessibles |
| Texte libre | `Stamp` + objets texte Helvetica + `/Contents` | explicite | `FreeText` serait invisible dans MegaUltraPDF (pas de génération, pas d'objets) |
| Tampon prédéfini | `Stamp` + cadre + texte centré + `/Contents` | explicite | — |

- **La couleur avant l'apparence** : `FPDFAnnot_SetColor` refuse une annotation qui a un
  `/AP`. Ordre imposé dans `add_annotation` : `/Rect`, couleurs, métadonnées, puis objets.
- **Modifier** (`UpdateAnnotation { patch }`) : les champs à changer partent vers Rust, qui
  reconstruit le reste depuis la lecture (`Annotation::to_new`) — aucune décision de
  reconstruction en TypeScript. Une **note** est modifiée sur place (contenu, auteur) ; sa
  couleur `/C` n'est ni relue (le symbole PDFium est jaune quoi qu'il arrive) ni réécrite.
  Les autres genres sont supprimés puis recréés après validation (une demande invalide ne
  retire rien). **Déplacer une forme = recréer**
  avec la géométrie remise à l'échelle entre les deux emprises intérieures (l'apparence est
  en coordonnées de page ; changer `/Rect` seul la laisse sur place ou la déforme selon
  les lecteurs). Une note (sans `/AP`) change simplement de `/Rect`. Les marquages ne se
  déplacent pas. L'index d'une annotation recréée change (fin de tableau) : l'interface
  resynchronise sa sélection après chaque édition.
- **Relecture par l'apparence** : le genre d'une `Ink`/`Stamp` et sa géométrie sont
  reconstruits en analysant les segments de ses objets chemin, avec les **signatures
  exactes de nos propres dessins** (12 points de Bézier fermés → ellipse ; 1 `MoveTo` +
  1 `LineTo` → ligne ; 3-4 segments axiaux → rectangle ; fût + pointe partant de son
  extrémité → flèche ; polylignes → encre) et ses objets texte (texte seul → texte libre).
  Tout chemin contenant une courbe de Bézier hors de ces signatures (encre tierce lissée)
  reste une **encre sans géométrie**, donc non déplaçable : la redessiner la transformerait.
  L'emprise est calculée par les **points** des segments, pas par `bounds()` (qui inclut la
  moitié du trait et ferait grossir la forme à chaque recréation). `Annotation.editable`
  (= recréable, ou note) dit à l'interface ce qu'elle peut déplacer ou modifier.
- **`/C` jamais lu directement** : `stroke_color()` / `fill_color()` de pdfium-render 0.9.4
  sur une annotation qui possède un `/AP` transtypent le handle en objet de page
  (comportement indéfini : plantage ou noir renvoyé au hasard), et PDFium génère un `/AP`
  pour presque tout dès le premier rendu. La couleur vient donc **uniquement** des objets
  d'apparence (remplissage pour une note, un surlignage ou une forme tierce ; trait pour nos
  formes) ; elle est inconnue (`None`) avant le premier rendu. Bornes de lecture : 2000
  annotations par page, 512 quadrilatères, 4096 caractères par chaîne.
- **Opacité** : pour nos apparences explicites (`Ink`, `Stamp`), l'alpha vit dans les objets
  (`ExtGState` du flux) et `/CA` reste à 1 : un lecteur qui applique `/CA` par-dessus
  l'apparence ne double pas la transparence. Les marquages gardent `/CA` (PDFium en
  déduit l'`ExtGState` de l'apparence qu'il génère).
- **Lecture** : un marquage sans apparence (fraîchement créé) déclenche un rendu minuscule
  de la page pour que PDFium génère l'apparence avant la lecture : la couleur est connue
  dès la création.
- **Enregistrement** : `save_as` recharge le document depuis ses octets avant d'écrire,
  ce qui écarte les objets orphelins (apparences de popup) que chaque rendu avec
  `FPDF_ANNOT` ajoute au document PDFium ; la mémoire de session repart aussi légère.
- Rendu avec `FPDF_ANNOT` (`render_annotations(true)`) : les annotations existantes
  apparaissent dans l'image de la page ; le calque SVG ne porte que l'interaction (zone des
  notes, poignées, aperçu du tracé).

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
- Pas d'`/InkList` (clé requise par la norme pour `Ink`) : le fichier est lisible partout mais
  non conforme ; dans Acrobat, modifier la couleur ou copier la forme régénère l'apparence
  depuis l'`/InkList` absent et la fait disparaître. À corriger au Lot 5 avec `lopdf`
  (ajout de `/InkList`, ou réécriture en vrais `Square`/`Circle`/`Line`).
- La couleur d'une note est celle de PDFium (jaune) dans tous les lecteurs tant que l'`/AP`
  généré subsiste ; `/C` est écrit mais sans effet. Le Lot 5 retirera `/AP` et
  `/PDFIUM_HasGeneratedAP` des notes à l'enregistrement.
- Chaque rendu avec `FPDF_ANNOT` crée des objets `Popup` orphelins dans le document PDFium
  (un par annotation à `/Contents` non vide) ; ils gonflent la mémoire et les instantanés
  d'historique pendant la session, mais plus le fichier enregistré (rechargement propre
  avant écriture). Élagage complet au Lot 5 (`lopdf::prune_objects`).
- Tests : `crates/mega-core/tests/annotations.rs` (lecture tierce, aller-retour par genre,
  déplacement, validation), `tests/edit.rs`, `tests/service.rs`, CLI `annots`, scénarios
  CDP `annotation.json` et `annotation-tierce.json`.
