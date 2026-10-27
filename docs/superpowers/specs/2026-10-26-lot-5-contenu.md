# Lot 5 — Édition de contenu : spécification

Statut : approuvée par mandat (exécution autonome). Référence : conception §5 Lot 5, ADR-0003
(double moteur), ADR-0007 (commandes), ADR-0008 / ADR-0009 (dettes `lopdf`).

## Intention

Toucher au **contenu** des pages : poser du texte et des images, déplacer et supprimer des
objets existants, retoucher un texte « au mieux », caviarder réellement (le contenu
disparaît du fichier), filigraner, numéroter, recadrer. Et solder les dettes des Lots 3 et 4
que seul `lopdf` peut régler : `/InkList` des formes, suppression de l'`/AP` imposé aux
notes, élagage des objets orphelins, pages tournées (`/Rotate`).

## Ce que les moteurs permettent

- **pdfium-render** (sûr) : objets de page (`objects()`, `iter`, `remove_object`,
  `create_text_object` avec polices intégrées Standard 14 ou TrueType chargée depuis des
  octets, `create_image_object`, `add_path_object`), transformation d'un objet
  (`translate`, `scale`, `rotate` via la matrice), `text()` d'un objet texte (lecture),
  `set_text` ; boîtes (`boundaries_mut().set_crop(...)`) ; `set_rotation` ; régénération du
  contenu (`regenerate_content`). Pas de « retouche » d'un texte existant au-delà de
  `set_text` sur l'objet (même police, même position).
- **lopdf** : flux de contenu (`get_page_content`, `Content::decode/encode`,
  `change_page_content`), objets (`get_object_mut`, `delete_object`, `prune_objects`,
  `renumber_objects`), dictionnaires d'annotations (`/InkList`, suppression de clés), `/Rotate`,
  `/CropBox`, ajout de flux (`add_page_contents`).

## Décisions (mes choix ; ADR-0010)

1. **Insertion** (texte, image) = objets PDFium ajoutés au contenu, comme la signature du Lot 4 ;
   texte : police Standard 14 (Helvetica, Times, Courier ; gras/italique), taille, couleur ;
   caractères hors WinAnsi → police TrueType **DejaVu Sans** embarquée dans le binaire
   (`include_bytes`, licence libre) ; image : PNG/JPEG, bornes du Lot 4.
2. **Sélection d'objets** : Rust expose les objets d'une page (`PageObject { index, kind:
   Text|Image|Path|Form|Shading|Other, bounds, text?: String }`) ; l'interface les met en
   surbrillance au survol (outil « Objets »), permet de **déplacer** (glisser → `translate`),
   **supprimer** (Suppr), **modifier le texte** (double-clic → `set_text`, « au mieux » : même
   police, taille et position ; si la police ne contient pas un glyphe, PDFium affiche un
   carré : avertissement affiché).
3. **Caviardage réel** : rectangle tracé → `lopdf` : les opérateurs de texte dont la boîte
   (calculée par PDFium : `text_boxes`) tombe dans la zone sont **retirés du flux de contenu**
   (par caractère si l'opérateur `TJ`/`Tj` le permet, sinon l'opérateur entier), les objets
   image entièrement couverts sont supprimés, puis un rectangle noir est dessiné ; les
   annotations de la zone sont supprimées. Documenté : les images partiellement couvertes
   sont **recouvertes**, pas rognées (limite) ; le texte caché mais présent dans les
   métadonnées (signets, champs) n'est pas touché. Test : extraction de texte après caviardage
   ne contient plus le mot.
4. **Filigrane** (texte en diagonale, opacité, taille, sur toutes les pages ou une plage) et
   **numérotation / en-tête / pied** (modèle `{n}` `{total}` `{titre}`, position, marge,
   police) = flux de contenu ajoutés par `lopdf` (`add_page_contents`, après le contenu,
   avec `q … Q` et `ExtGState` pour l'opacité) : indépendants de PDFium, exacts partout.
5. **Recadrage** = `/CropBox` (`boundaries_mut().set_crop`) par page ou plage ; interface :
   rectangle tracé ou marges numériques ; « réinitialiser » remet la MediaBox.
6. **Dettes** : `/InkList` écrit pour nos `Ink` (polygone approché pour rectangle/ellipse/
   ligne/flèche, points pour l'encre) ; `/AP` et `/PDFIUM_HasGeneratedAP` retirés des notes
   à l'enregistrement ; `prune_objects` + `renumber_objects` à l'enregistrement (`save_as`
   passe par `lopdf` avant l'écriture) ; **pages tournées** : les commandes d'annotation,
   de signature et de contenu reçoivent des coordonnées dans l'espace **affiché** et sont
   converties vers l'espace utilisateur non tourné (`/Rotate`) en Rust (`geometry::unrotate`).
7. Toute édition = `EditCommand` (annulable) : `InsertText`, `InsertImage`, `MoveObject`,
   `DeleteObject`, `SetObjectText`, `Redact`, `Watermark`, `PageNumbers`, `CropPages`.
8. CLI : `mega watermark`, `mega number`, `mega crop`, `mega redact --pages --rect`.

## Hors périmètre

Retouche typographique avancée (reflow, césure), OCR, export (Lot 6), métadonnées (Lot 6).

## Fixtures

`sample_pdf` suffit pour texte/images ; `testing::rotated_pdf()` (`/Rotate 90`) ;
`testing::rich_pdf()` (texte sur deux lignes + image + chemin) pour le caviardage ;
réutilisation d'`annotated.pdf` pour les dettes.

## Concepts Rust

`include_bytes!` et ressources embarquées, matrices affines (`PdfMatrix`), parsing/réécriture
d'un flux d'opérateurs (`lopdf::content`), traits pour abstraire « où écrire » (PDFium ou lopdf),
tests d'invariants (texte caviardé absent de l'extraction).

## Checklist CDP (`contenu.json`)

1. Outil Texte-contenu : clic, saisie « Bonjour », taille 18 → visible, listé comme objet.
2. Outil Objets : survol met en surbrillance « Page 1 », glisser → déplacé, Suppr → disparu, Ctrl+Z.
3. Double-clic sur un texte → saisie « Page une » → rendu change.
4. Image : insérer `fixtures/logo.png` → visible, redimensionnable.
5. Caviarder le mot « Page » → `mega text` ne le trouve plus, rectangle noir visible.
6. Filigrane « BROUILLON » sur toutes les pages ; numérotation « {n} / {total} » en pied.
7. Recadrer la page 1 (marges 50 pt) → dimensions changées dans les propriétés.
8. Ouvrir une page tournée : annotation posée au bon endroit après réouverture.
