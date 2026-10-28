# Lot 10 — Fonctionnalités restantes

Date : 2026-10-28. Plan approuvé par l'utilisateur (périmètre : police Unicode, caviardage
d'images, signature visible, finitions techniques ; une release v0.10.0 à la fin du lot).

## Intention

Combler les quatre derniers manques de la liste des fonctionnalités d'un logiciel PDF :

1. écrire du texte **hors de l'alphabet WinAnsi** (grec, cyrillique, latin étendu : « Łódź »,
   « Ελλάδα », « Привет ») partout où l'application écrit du texte ;
2. caviarder une **partie d'image** sans laisser les pixels dans le fichier ;
3. poser une **signature numérique visible** (cadre avec nom, date, image facultative) ;
4. finir les dettes techniques : nettoyage des documents **chiffrés**, OCR annulable en **un seul
   pas**, gestion des dialogues de l'interface, mineurs reportés du Lot 9.

## Périmètre

### A. Police Unicode incorporée

- Police : **DejaVu Sans 2.37** (licence Bitstream Vera + domaine public, compatible GPL),
  fichier `crates/mega-core/assets/fonts/DejaVuSans.ttf` (`include_bytes!`) et sa licence
  `assets/fonts/LICENSE-DejaVu.txt`.
- Dépendances nouvelles : `ttf-parser` 0.25 (lecture `cmap`/`hmtx`, `default-features = false`,
  feature `std`) et `subsetter` 0.2 (`default-features = false`) pour n'incorporer que les glyphes
  utilisés par nos flux `lopdf`.
- **Helvetica WinAnsi reste le chemin par défaut** ; la police Unicode ne sert que pour un texte
  non encodable en WinAnsi. Les fichiers produits aujourd'hui restent identiques.
- Écriture `lopdf` (filigrane, numérotation, couche OCR, apparence de signature) : police
  `Type0` / `Identity-H`, `CIDFontType2` sous-ensemble, `/W`, `/CIDToGIDMap` (flux),
  `/ToUnicode`. CID attribués dans l'ordre d'apparition des caractères.
- Écriture PDFium (texte inséré, retouche de texte, texte libre et tampon) :
  `fonts_mut().load_true_type_from_bytes(DEJAVU, true)` ; la police entière est incorporée par
  PDFium (pas de sous-ensemble possible : PDFium exige une `cmap`) ; la passe
  `cleanup::before_save` **fusionne les programmes de police identiques** pour qu'il n'en reste
  qu'un par fichier.
- Caractère **visible** absent de DejaVu (CJK, etc.) : refus explicite listant les caractères.
  Caractère absent dans la couche **invisible** OCR : accepté (glyphe 0, retrouvé par
  `/ToUnicode` pour la recherche et la copie).
- Pas de mise en forme complexe (arabe lié, scripts indiens) : hors périmètre, documenté.
- Formulaires : une valeur de champ en cyrillique est écrite en UTF-16BE+BOM (`/V`).

### B. Caviardage d'images et de tracés

- Image **partiellement** sous la zone : les pixels de la zone (projetée par l'inverse de la
  matrice de l'image, arrondie vers l'extérieur + 1 px) deviennent noirs et opaques ; l'objet est
  remplacé par une **nouvelle** image (même matrice, même position dans la liste d'objets). Une
  image partagée par une autre page y reste intacte.
- Image entièrement couverte : supprimée (comme aujourd'hui).
- Tracé contenu dans la zone : supprimé. Tracé qui traverse la zone : conservé sous le cache,
  compté (`paths_kept`) et signalé dans l'interface.
- La page caviardée perd `/Thumb`, `/PieceInfo` et `/Metadata`.
- Image dans un XObject de formulaire : refus explicite (inchangé).
- Limite : une image CMYK, indexée ou 16 bits est réécrite en RGB 8 bits.

### C. Signature visible

- `SignOptions.appearance: Option<SignAppearance>` : page, rectangle (espace affiché), image
  PNG/JPEG facultative, texte (nom du signataire, date, motif).
- Le widget reçoit `/Rect` réel et une apparence `/AP /N` ; la page référence le widget.
  Une page tournée affiche le cadre droit (`/Matrix` de l'apparence).
- Interface : case « Signature visible » dans le dialogue de signature, rectangle tracé avec
  l'outil signature existant, image facultative.

### D. Finitions

1. `cleanup::before_save(bytes, password)` nettoie aussi un document chiffré (RC4, AES-128,
   AES-256) puis le **rechiffre à l'identique** (même clé, `/O`, `/U`, `/P`, `/ID`). Le
   caviardage, le filigrane, la numérotation et la couche OCR deviennent possibles sur un
   document chiffré ouvert avec son mot de passe.
2. L'OCR de N pages est **un seul pas** d'annulation (« Reconnaître le texte (N pages) »).
3. `+page.svelte` : un seul `openDialog` remplace les booléens de dialogue ; composant
   `MenuButton` partagé par le menu Fichier et la barre de contenu.
4. Mineurs reportés du Lot 9 (liste dans le plan), chacun jugé à l'effet.

### Hors périmètre

Mise en forme complexe des écritures, polices CJK, choix de police par l'utilisateur au-delà de
« Unicode (DejaVu Sans) », caviardage d'images dans un XObject de formulaire, signature d'un
document chiffré, horodatage RFC 3161.

## API ajoutées

`mega-core` :
- `fonts::unicode::{UnicodeFont, DEJAVU_SANS}` (crate-interne sauf `UnicodeFont::missing`
  exposé via `fonts::missing_glyphs(&str) -> Vec<char>` pour le CLI et l'interface).
- `content::FontFamily::Unicode`.
- `content::redact::RedactReport { texts, images_removed, images_redacted, paths_removed,
  paths_kept, annotations }`.
- `digital_signature::{SignAppearance}` ; `SignOptions.appearance`.
- `cleanup::before_save(bytes: &[u8], password: Option<&str>)`.
- `Document::begin_edit_group(label) / end_edit_group()` ; `HistoryState.group_open`.

Tauri (`commands.rs`) : `begin_edit_group`, `end_edit_group` ; `sign_document` reçoit
`appearance`. `api.ts` reflète ces types.

## Concepts Rust travaillés

`include_bytes!` et données statiques ; `OnceLock` pour une analyse faite une fois ; lecture de
tables binaires via une bibliothèque (`ttf-parser`) ; `BTreeMap` comme table d'attribution
(CID) ; matrices et inversion (géométrie) ; un « groupe » dans un historique (état explicite) ;
`enum` d'état de l'interface (TypeScript, union discriminée).

## Fixtures

- `fixtures/image-mosaic.pdf` (`cargo xtask gen-fixtures`) : une page avec une image 64×64 à
  quatre quadrants de couleurs connues et un motif d'octets unique, plus une image avec SMask ;
  une seconde page réutilisant la même image.
- Fixtures existantes : `encrypted.pdf`, `owner-only.pdf`, `rotated.pdf`, `scanned.pdf`,
  `test-signer.p12`.

## Checklist de validation manuelle

- Filigrane « Привет мир » : visible, la recherche le trouve, copie correcte.
- Numérotation « Страница {n} » ; texte inséré en grec ; tampon « Ελλάδα ».
- Texte « 你好 » : refus listant les caractères.
- Caviardage d'un quart d'image : seul ce quart est noir ; le fichier enregistré ne contient
  plus les pixels.
- Signature visible sur une page tournée : cadre droit, panneau « Intacte », pyHanko valide.
- Document chiffré : filigrane puis enregistrement ; réouverture avec le même mot de passe.
- OCR de 3 pages puis Ctrl+Z : un seul pas.
- Dialogues : un seul à la fois, Échap ferme, le focus revient.
