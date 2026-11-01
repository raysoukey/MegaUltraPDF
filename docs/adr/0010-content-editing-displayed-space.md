# ADR-0010 — Édition de contenu : espace affiché, objets PDFium, flux `lopdf`, caviardage par objets

Date : 2026-10-26 · Statut : accepté (Lot 5)

## Contexte

Le Lot 5 touche au **contenu** des pages (texte, images, tracés), là où les lots précédents
posaient des annotations ou remplissaient des champs. PDFium (pdfium-render 0.9.4, sans
`unsafe`) expose les objets de page (lecture, suppression, transformation, `set_text`,
création de texte et d'images) mais ni l'édition d'un flux de contenu existant, ni l'ajout
de ressources partagées, ni le nettoyage de ce qu'il laisse dans le fichier (clé privée
`/PDFIUM_HasGeneratedAP`, objets orphelins). Les pages portant un `/Rotate` étaient affichées
tournées mais toute l'API parlait en coordonnées du fichier : une annotation posée « ici » à
l'écran atterrissait ailleurs.

## Décisions

1. **Un seul espace de coordonnées pour toute l'API : l'espace affiché.** `geometry::PageFrame`
   (`/Rotate` + zone visible CropBox ∩ MediaBox) convertit vers l'espace utilisateur du fichier
   aux seuls endroits où les coordonnées entrent ou sortent de PDFium : annotations, champs,
   signature, objets, texte, liens, recherche. Les textes et images que nous dessinons sur une
   page tournée sont mis en page en espace affiché puis **tournés par la matrice du repère**
   (`cm` dans un flux, `apply_matrix` sur un objet) : ils restent droits à l'écran. L'interface
   n'a rien à savoir de `/Rotate`.
2. **Objets de page = API PDFium**, index positionnel (comme `/Annots`), relus après chaque
   édition : `page_objects`, `delete_object`, `move_object` (vecteur en axes affichés),
   `set_object_text` (même police, même position : retouche « au mieux »), `insert_text`
   (Standard 14, WinAnsi, une ligne par objet) et `insert_image` (chargement borné partagé avec
   la signature). Les caractères hors WinAnsi sont **refusés** avec un message explicite plutôt
   que rendus par un carré ; embarquer une police reste possible plus tard.
3. **Caviardage réel à la granularité de l'objet** : tout objet texte ou image et toute
   annotation qui **touche** la zone est retiré du fichier, puis un rectangle noir est dessiné.
   On enlève parfois plus que demandé (un objet texte est souvent une ligne entière) : c'est
   le bon côté où se tromper. Chemins et formulaires XObject sont recouverts, pas retirés (un
   XObject pleine page disparaîtrait sinon avec lui le reste de la page). Le texte retiré n'est
   plus dans l'extraction ni dans les octets.
4. **Filigrane et numérotation = flux de contenu écrits par `lopdf`**, ajoutés après le contenu
   existant (enfermé une fois dans `q … Q`), avec une police Helvetica/WinAnsi et un
   `ExtGState` d'opacité posés dans des ressources **propres à la page** (les ressources
   héritées sont copiées, les sous-dictionnaires partagés dé-partagés). PDFium ne sert qu'à
   mesurer la largeur du texte et à recharger le document. Résultat identique dans tous les
   lecteurs. Le recadrage est un `/CropBox` (PDFium), par marges affichées depuis la MediaBox.
5. **Passe de nettoyage à l'enregistrement** (`cleanup::before_save`, `lopdf`) : `/InkList`
   déduite de l'apparence pour nos `Ink`, `/AP` imposé aux notes et `/PDFIUM_HasGeneratedAP`
   retirés, objets orphelins élagués et renumérotés. Un fichier chiffré est nettoyé puis
   rechiffré à l'identique (Lot 10, ADR-0014 §7). Un fichier illisible par `lopdf` est écrit tel
   quel, **sauf** après une suppression : l'écriture est alors refusée (Lot 14, ADR-0019 §1).
6. Toute édition reste une `EditCommand` appliquée par `Document::apply` (ADR-0007) ; la CLI
   et l'application exécutent le même code, et écrivent par `save_as` (passe de nettoyage
   comprise), jamais par `to_bytes()` brut.
7. **Refus plutôt que promesse fausse** (relectures de fin de lot) : l'édition d'objets et le
   caviardage sont refusés quand `lopdf` ne pourrait pas élaguer ce qu'ils retirent (les objets
   retirés resteraient en orphelins, car `FPDF_SaveAsCopy` recopie tout). Au Lot 5, cela
   visait tout PDF chiffré ; le Lot 10 a levé ce refus, et depuis le Lot 14 il vise tout
   fichier que `lopdf` ne relit pas (`Document::ensure_cleanable`, ADR-0019 §1). Ils sont
   refusés aussi sur une page dont PDFium régénérerait le flux en perdant du contenu (police
   Type 3, image en ligne, ombrage : `content::check_regeneration_safe`), quand la zone touche
   un champ de formulaire (sa valeur resterait dans `/AcroForm`) et quand du texte reste sous
   le cache (dessiné via un XObject : vérification par `text_boxes` après retrait, instantané
   restauré). La réécriture `lopdf` est validée (`rewrite_is_sane`) avant de remplacer les
   octets PDFium. Au Lot 5, toute passe `lopdf` (nettoyage, filigrane, champs) s'abstenait sur
   un fichier chiffré ou déchiffré en silence (`was_encrypted()`) ; depuis le Lot 10, elles
   passent par `crypt::LopdfSession`, qui déchiffre puis rechiffre à l'identique (ADR-0014 §7),
   en passe-plat pour des filtres `Identity` (ADR-0019 §2).

## Conséquences

- Les tests de géométrie pivotent systématiquement la fixture `rotated.pdf` (400 × 600,
  `/Rotate 90`) : annotations, champs, signature, objets, filigrane, recadrage.
- `scaled_font_size()` de pdfium-render lit le coefficient `d` de la matrice (nul pour un
  texte tourné) : la taille effective est la norme de la colonne `(b, d)`
  (`content::effective_font_size`).
- Deux limites documentées dans l'interface par les libellés : la retouche de texte ne
  recompose pas, le caviardage retire des objets entiers.
- Dettes reportées : police embarquée (DejaVu) pour l'Unicode hors WinAnsi, caviardage par
  caractère dans le flux et caviardage raster des images (un scan pleine page est retiré en
  entier), recadrage par rectangle tracé, popups orphelins après suppression d'une note
  tierce, avertissement quand une réécriture invalide une signature numérique.

## Mise à jour (Lot 14, 2026-10-31)

Audit de compatibilité `docs/audit/2026-10-31-compatibilite-pdf.md`, constats E5, E6 et E9.

- **Balisage après suppression d'un objet** (E5) : `regenerate_content` garde les marques
  (`BDC`/`EMC`, `/MCID`, `/OC`) des objets qui restent ; seule celle de l'objet supprimé
  disparaît. La passe d'enregistrement retire alors de l'arbre de structure le MCID devenu
  absent (`cleanup/mcid.rs`, règle conservatrice : ADR-0019 §6).
- **Filigrane, numéros et couche OCR** (E6) : enveloppés dans `/Artifact BMC … EMC` (la
  numérotation en `/Artifact <</Type /Pagination>> BDC`), pour qu'un lecteur d'écran les
  ignore. Sur un document qui déclare PDF/A ou PDF/UA, le texte prend la police Unicode
  incorporée même en WinAnsi (Helvetica n'est pas incorporée). En PDF/A, la couleur est écrite
  en `CalRGB` ; en PDF/A-1, sans `/Group` ni opacité, la transparence étant simulée en
  éclaircissant la couleur (ADR-0018 §7).
- **Image insérée** (E9) : écrite par `lopdf` et non plus par PDFium ; un JPEG garde ses
  octets (`/DCTDecode`), un masque `/SMask` n'est ajouté qu'en cas de vraie transparence
  (ADR-0019 §10). La mise en place reste celle du §1 : espace affiché, puis matrice.
