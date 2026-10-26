# Lot 3 — Annotations : spécification

Statut : approuvée par mandat (exécution autonome). Référence : conception §5 Lot 3, ADR-0006
(acteur), ADR-0007 (commandes d'édition, memento, écriture atomique).

## Intention

Permettre d'**annoter** un PDF comme on le fait avec un stylo sur une feuille : surligner,
souligner, barrer un passage, poser une note, écrire un texte, dessiner à main levée, tracer
un rectangle, une ellipse, une ligne ou une flèche, apposer un tampon ; puis retrouver,
déplacer, redimensionner, supprimer ces annotations, et les enregistrer de façon **lisible
par les autres lecteurs** (Acrobat, PDF.js, SumatraPDF, Aperçu). Chaque action est une
`EditCommand` : annulable, rétablissable, marquée « modifié », enregistrée atomiquement.

## Ce que dit la norme (ISO 32000-2 §12.5), en bref

- Une annotation est un dictionnaire dans le tableau `/Annots` de la page : `/Subtype`,
  `/Rect` (coordonnées PDF, origine en bas à gauche, points), `/F` (drapeaux : Print = 4),
  `/Contents`, `/NM` (nom unique), `/M` (date `D:YYYYMMDDHHmmSS+HH'mm'`), `/C` (couleur
  RGB du trait ou de l'icône), `/CA` (opacité 0-1), `/AP` (apparence : flux `/N` obligatoire
  pour un rendu fidèle), `/P`.
- Annotations de **marquage** (§12.5.6.2) : `/T` (auteur), `/CreationDate`, `/Subj`,
  `/Popup`, `/IRT`. Les marquages de texte (`Highlight`, `Underline`, `StrikeOut`,
  `Squiggly`) portent `/QuadPoints` (8 nombres par quadrilatère, ordre x1 y1 x2 y2 x3 y3 x4 y4,
  haut-gauche, haut-droit, bas-gauche, bas-droit).
- `Square`/`Circle` : `/IC` (couleur de remplissage), `/BS` (`/W` épaisseur). `Line` : `/L`,
  `/LE` (extrémités, flèche = `/OpenArrow`). `Ink` : `/InkList` (tableaux de points).
  `FreeText` : `/DA` (chaîne d'apparence par défaut), `/Q`. `Text` : `/Name` (icône),
  `/Open`. `Stamp` : `/Name` (ex. `/Approved`) et surtout un `/AP`.
- Un lecteur **doit** utiliser `/AP` s'il existe ; sans `/AP`, il *peut* dessiner une
  apparence par défaut (Acrobat, PDF.js, MuPDF le font pour la plupart des sous-types ;
  **PDFium ne le fait que pour** Square, Circle, Highlight, Ink, Popup, Squiggly, StrikeOut,
  Underline — pas pour Text, FreeText, Line, Stamp).
- Chaînes : PDFDocEncoding ou UTF-16BE avec BOM (pdfium-render encode en UTF-16 via
  `FPDFAnnot_SetStringValue`) ; dates au format `D:`.

## Ce que `pdfium-render` 0.9.4 permet **sans `unsafe`** (contrainte `#![forbid(unsafe_code)]`)

- Création : `create_text_annotation`, `create_highlight/underline/strikeout/squiggly_annotation`
  (+ `attachment_points_mut().create_attachment_point_at_end(PdfQuadPoints)`),
  `create_square_annotation`, `create_ink_annotation`, `create_stamp_annotation`,
  `create_free_text_annotation`, `create_popup_annotation`, `delete_annotation`.
  **Pas** de création `Circle`, `Line`, `Polygon`.
- Communs : `bounds/set_bounds`, `contents/set_contents`, `creator/set_creator` (= `/T`),
  `creation_date/set_creation_date` (`chrono::DateTime<Utc>`), `modification_date`,
  `stroke_color/set_stroke_color` (`/C` + `/CA` via l'alpha), `fill_color/set_fill_color`
  (`/IC`), drapeaux (`set_is_printed`…), `name` (`/NM`, lecture seule).
- `FPDFAnnot_SetColor` **échoue si l'annotation a déjà un `/AP`** : les couleurs se posent
  à la création, avant l'apparence ; modifier une annotation = la supprimer et la recréer.
- Apparence personnalisée : `objects_mut()` (**Ink et Stamp seulement**, limite de
  `FPDFAnnot_AppendObject`) accepte des objets chemin (`PdfPagePathObject` : lignes,
  Bézier, rectangle, ellipse, couleur et épaisseur de trait, remplissage) et texte
  (`PdfPageTextObject` avec police intégrée Helvetica).
- Non exposé : `FPDFAnnot_AddInkStroke` (`/InkList`), `FPDFAnnot_SetBorder` (`/BS`),
  `FPDFAnnot_SetAP`, `/Subj`, `/Name` d'icône.
- Rendu : `PdfRenderConfig::render_annotations(true)` (drapeau `FPDF_ANNOT`), **absent
  aujourd'hui** dans `render.rs` : les annotations existantes ne sont pas affichées.

## Décisions (mes choix, consignées ; ADR-0008)

1. **Sous-types utilisés** :
   - Note → `Text` (contenu, auteur, dates). PDFium ne dessine pas d'icône : le calque SVG de
     l'application dessine l'icône de note **en permanence** (pas seulement en édition) ;
     les autres lecteurs affichent leur icône par défaut.
   - Surligner / souligner / barrer / ondulé → `Highlight` / `Underline` / `StrikeOut` /
     `Squiggly` avec `/QuadPoints` issus de la sélection de texte et `/C` ; PDFium génère
     l'`/AP` au premier rendu, les autres lecteurs savent les dessiner sans.
   - Rectangle, ellipse, ligne, flèche, encre → **`Ink` avec apparence vectorielle**
     (objets chemin : épaisseur, couleur, opacité, remplissage). Faute de création
     `Circle`/`Line`/`/BS` sans `unsafe`, c'est le seul moyen d'obtenir un rendu identique
     partout ; le sous-type réel (rectangle, ellipse…) est conservé dans `/Contents` sous la
     forme `megaultrapdf:shape=rect` **uniquement si** le contenu est vide ? Non : on garde
     `/Contents` pour l'utilisateur. Le type de forme est **déduit à la lecture** de la
     géométrie de l'objet chemin (rectangle, ellipse, segment avec ou sans pointe) ; à défaut,
     « encre ». Révision possible au Lot 5 avec `lopdf` (vrais `Square`/`Circle`/`Line`).
     Pas d'`/InkList` : un lecteur qui régénère l'apparence depuis `/InkList` (rare, à
     l'édition seulement) perdrait le tracé ; documenté.
   - Texte libre → **`Stamp` avec apparence texte** (objet texte Helvetica, taille, couleur)
     et `/Contents` = le texte. `FreeText` est écarté : PDFium n'en génère pas l'apparence
     et `FPDFAnnot_AppendObject` le refuse, le texte serait invisible dans MegaUltraPDF.
   - Tampon prédéfini (« APPROUVÉ », « BROUILLON », « CONFIDENTIEL », « REFUSÉ », « VU ») →
     `Stamp` avec apparence vectorielle (cadre + texte), `/Contents` = libellé.
2. **Modèle** : `mega_core::annotations` expose `Annotation { index, kind, rect, color,
   opacity, stroke_width, author, contents, created, modified, quads, points }` (lecture) et
   `NewAnnotation { kind: AnnotationKind, geometry: Geometry, style: Style, contents, author }`
   (création), tous deux `serde` (miroir `api.ts`). `AnnotationKind` = `Note | Highlight |
   Underline | StrikeOut | Squiggly | Rect | Ellipse | Line | Arrow | Ink | Text | Stamp |
   Other(String)`.
3. **Commandes** (`EditCommand`, ADR-0007) : `AddAnnotation { page, annotation }`,
   `DeleteAnnotation { page, index }`, `UpdateAnnotation { page, index, annotation }`
   (supprimer puis recréer, même position d'index non garantie : l'interface se resynchronise
   par relecture), `MoveAnnotation { page, index, rect }` (`set_bounds` ; pour une apparence
   `/AP`, PDF applique la matrice BBox→Rect : déplacer/redimensionner met à l'échelle).
4. **Auteur** : préférence `author` dans `settings.json` (défaut : nom d'utilisateur du
   système, saisissable dans les préférences du Lot 7 ; pour ce lot, un champ dans le panneau
   des annotations).
5. **Calque d'édition SVG** par page (au-dessus du calque de texte, `pointer-events` selon
   l'outil) : affichage des icônes de note, des poignées de sélection, de l'aperçu pendant le
   tracé. Les **annotations elles-mêmes** (sauf notes) sont rendues par PDFium dans l'image de
   la page (`render_annotations(true)`), donc fidèles à ce que verront les autres lecteurs.
6. **Outils** (machine à états dans `app/src/lib/logic/tools.ts`, pure) : `select`, `note`,
   `highlight`, `underline`, `strikeout`, `text`, `ink`, `rect`, `ellipse`, `line`, `arrow`,
   `stamp`. Les marquages de texte s'appliquent à la **sélection de texte courante** (bouton
   ou raccourci) ou par glisser avec l'outil actif. Échap revient à `select`.
7. **Lecture ailleurs** : vérifiée par un test qui rouvre le fichier enregistré avec PDFium
   (rendu non vide à l'emplacement de l'annotation) et par la relecture `pdf-expert` ; les
   fixtures tierces (`annotated.pdf`, écrit à la main avec `/AP`) couvrent la lecture.

## Périmètre (ce qui entre) — `docs/FONCTIONNALITES.md`, section « Annotations »

- Créer : surligner, souligner, barrer (depuis la sélection de texte), note, texte, encre,
  rectangle, ellipse, ligne, flèche, tampons prédéfinis.
- Propriétés à la création et modification : couleur (palette + personnalisée), opacité,
  épaisseur, taille de police (texte), auteur, contenu ; dates automatiques.
- Sélectionner (clic sur le calque), déplacer, redimensionner (poignées), supprimer (Suppr),
  modifier le contenu (double-clic) ; panneau latéral « Annotations » (liste par page : type,
  auteur, date, extrait ; clic = aller à l'annotation ; supprimer).
- Rendu des annotations existantes dans les pages (`render_annotations`).
- Tout passe par `EditCommand` → annuler/rétablir, « • », enregistrement atomique.
- CLI : `mega annots <fichier> [--page N]` (liste), banc d'essai du modèle de lecture.

## Hors périmètre

Pièces jointes, sons, vidéos, 3D, commentaires en réponse (`/IRT`), popups séparées,
`/InkList`, `FreeText` réel, caviardage (Lot 5), formulaires (Lot 4), import/export FDF/XFDF,
rotation des annotations avec la page tournée (les coordonnées suivent la page ; l'affichage
tourné du Lot 1 est géré par la géométrie existante).

## Architecture ajoutée

- `mega-core::annotations` : types ci-dessus ; `Document::annotations(page) -> Vec<Annotation>`,
  `Document::add_annotation(page, &NewAnnotation) -> u16` (index), `delete_annotation(page, index)`,
  `set_annotation_rect(page, index, Rect)` ; conversions `Color <-> PdfColor`, quads ;
  constructeur d'apparence (`appearance.rs`) : chemins pour rect/ellipse/ligne/flèche/encre,
  texte pour texte libre et tampons ; **détection de forme** à la lecture.
- `mega-core::edit` : 4 variantes `EditCommand` + libellés (« Ajouter un surlignage »,
  « Supprimer une note », « Déplacer une annotation », « Modifier une annotation »).
- `mega-core::render` : `render_annotations(true)`.
- `mega-core::service` : requête `Annotations { id, page }`.
- `mega-cli` : `annots`.
- Tauri : commande `annotations(id, page)` ; `api.ts` : `Annotation`, `NewAnnotation`,
  `AnnotationKind`, `annotations()`.
- Frontend : `logic/tools.ts` (machine à états, géométrie du tracé en coordonnées PDF),
  `logic/annotations.ts` (poignées, test de hit, conversions), `state/annotations` dans
  `documents.svelte.ts` (cache par page invalidé par `afterEdit`, sélection courante, outil
  actif, style courant), `components/AnnotationLayer.svelte`, `AnnotationToolbar.svelte`,
  `AnnotationPanel.svelte` (onglet du panneau latéral), `StampPicker.svelte`,
  `AnnotationEditor.svelte` (contenu/texte, propriétés), chaînes dans `strings.ts`,
  raccourcis (H surligner, U souligner, S barrer ? conflit… voir plan : `Alt+…` évité ;
  propositions : `H`, `U`, `K` barrer, `N` note, `T` texte, `D` dessiner, `Suppr`).

## Fixtures

- `testing::annotated_pdf()` : 1 page A4 avec une note `Text` (`/Contents`, `/T`,
  `/CreationDate`), un `Highlight` avec `/QuadPoints` et `/C`, un `Square` avec `/AP`
  (flux de forme) : lecture, détection, rendu.
- `fixtures/annotated.pdf` généré par `cargo xtask gen-fixtures` à partir du précédent
  (pour la CLI et les scénarios CDP).

## Concepts Rust travaillés (niveau : Lot 2 acquis)

`enum` riche + `match` exhaustif (sous-types), pattern **builder** (apparence, `NewAnnotation`),
conversions `From`/`TryFrom` entre nos types et ceux de pdfium-render, `Option` et
`filter_map` sur des données partielles, dates `chrono` (nouvelle dépendance : déjà dans
l'arbre via pdfium-render), tests d'aller-retour (écrire → réouvrir → relire), géométrie
(coordonnées PDF ↔ écran, déjà vue au Lot 1, étendue aux poignées). Côté TS : machine à états
pure pour les outils.

## Checklist de validation (CDP, `annotation.json`, sur une copie de `simple.pdf`)

1. Sélectionner « Page » en page 1, bouton Surligner → image de page rerendue avec la bande
   jaune ; panneau Annotations : 1 entrée « Surlignage ».
2. Outil Note, clic → icône dans le calque ; double-clic → saisie « Bonjour » ; panneau : extrait.
3. Outil Rectangle, glisser → rectangle rouge visible dans l'image ; poignées ; redimensionner
   → image mise à jour ; Suppr → disparaît ; Ctrl+Z → revient.
4. Outil Encre, tracé de 3 segments → visible ; Outil Flèche → visible.
5. Outil Texte, glisser puis taper « Essai » → texte visible dans l'image.
6. Tampon « BROUILLON » → visible ; panneau : 7 entrées.
7. Ctrl+S ; `mega annots copie.pdf` liste 7 annotations avec types, auteur et dates ;
   `mega render` de la page montre les annotations (pixels non blancs aux emplacements).
8. Ouvrir `fixtures/annotated.pdf` : annotations tierces rendues et listées.
