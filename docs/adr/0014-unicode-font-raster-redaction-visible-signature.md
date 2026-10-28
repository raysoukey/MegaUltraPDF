# ADR-0014 — Police Unicode incorporée, caviardage des images, signature visible, réécriture des documents chiffrés

Date : 2026-10-28 · Statut : accepté (Lot 10)

## Contexte

Quatre manques restaient après le Lot 9 :

- tout texte écrit par l'application passait par Helvetica, encodage WinAnsi : le grec, le
  cyrillique et le latin étendu (« Łódź ») étaient refusés ou remplacés par `?` ;
- un caviardage laissait sous son cache noir les pixels d'une image partiellement couverte ;
- la signature numérique n'avait qu'un widget invisible ;
- les réécritures `lopdf` (nettoyage, filigrane, caviardage…) s'abstenaient ou refusaient sur
  un document chiffré.

## Décisions

1. **DejaVu Sans 2.37 incorporée** (`include_bytes!`, licence Bitstream Vera + domaine public,
   citée dans `THIRD_PARTY_LICENSES.md` et « À propos »). Lecture des tables par `ttf-parser`
   (`fonts::unicode::UnicodeFont`, analysée une fois dans un `OnceLock`).
2. **Helvetica reste le chemin par défaut** : la police Unicode ne sert qu'à un texte hors
   WinAnsi. Les fichiers produits jusqu'ici restent identiques.
3. **Écriture `lopdf`** (filigrane, numéros, couche OCR, tampon de signature) : police
   `Type0` / `Identity-H` écrite par `Type0Writer`. Les CID sont attribués dans l'ordre
   d'apparition. Elle comporte un `CIDFontType2` **sous-ensemble** (`subsetter` : quelques Ko
   au lieu de 750), un `/W`, un `/CIDToGIDMap` en flux et un `/ToUnicode`. Un caractère
   absent de DejaVu (CJK) est refusé dans un texte visible, mais permis dans la couche
   invisible : il pointe vers le glyphe 0 et reste trouvable par `/ToUnicode`.
4. **Écriture PDFium** (texte inséré, retouche, texte libre, tampon) :
   `load_true_type_from_bytes(DejaVu, true)`. PDFium exige une `cmap`, donc il incorpore la
   police entière. Le jeton est gardé par `Document` jusqu'au prochain rechargement, et
   `cleanup::before_save` fusionne les programmes de police identiques. Un objet texte en
   police standard retouché avec ces lettres est remplacé par un objet DejaVu (même matrice).
5. **Caviardage des images par `lopdf`** (`content/raster.rs`), après la phase PDFium :
   - l'image est repérée par la matrice courante (`cm`) du flux ;
   - elle est retirée si la zone la couvre, sinon remplacée par une **nouvelle** image ;
   - dans la nouvelle image, les pixels sous la zone (projetée par la matrice inverse,
     arrondie vers l'extérieur + 1 px) sont noirs, et opaques dans le `/SMask` ;
   - l'espace de couleur et le reste du dictionnaire sont gardés.

   PDFium aurait perdu le masque et l'espace de couleur. Une image partagée reste intacte sur
   les autres pages. Une image indécodable (JPX, fax, 16 bits, `/Decode`, `/Mask`) est retirée
   entière. Une image dessinée dans un formulaire XObject est refusée.

   Un tracé contenu dans la zone est retiré ; un tracé qui la traverse est recouvert et
   compté (`paths_kept`, signalé à l'utilisateur). `/Thumb`, `/PieceInfo` et `/Metadata` de
   la page partent.
6. **Signature visible** : `SignOptions.appearance` (page, rectangle affiché, image).
   - Le widget reçoit son `/Rect` réel et une apparence `/AP /N`, écrite dans la même mise à
     jour incrémentale.
   - Sur une page tournée, la `/BBox` a les dimensions affichées et la `/Matrix` porte la
     rotation : le tampon reste droit.
   - L'interface propose un coin de la page courante ; le CLI accepte `--rect`.
7. **Session `lopdf` chiffrée** (`crypt::LopdfSession`) :
   - le document est chargé avec son mot de passe, ou le mot de passe vide pour une
     protection « propriétaire seul » ;
   - l'état de chiffrement est retenu ;
   - la passe travaille en clair ;
   - le document est **rechiffré avec le même état** : même clé, mêmes `/O`, `/U`, `/P` et
     même `/ID`.

   Le nettoyage, le filigrane, la numérotation, la couche OCR, le caviardage, la retouche
   d'objets, les formulaires et les métadonnées acceptent les documents chiffrés. La
   signature d'un document chiffré reste refusée.
8. **OCR en un pas d'annulation** : `Document::begin_edit_group` / `end_edit_group`.
   - La première édition du groupe dépose le seul instantané ; annuler est refusé tant que le
     groupe est ouvert.
   - L'interface garde la boucle page par page (progression, arrêt) et ferme toujours le
     groupe.
9. **Vérification SHA-384 / SHA-512** des signatures tierces (`cms::verify_with`, RSA et
   ECDSA P-256) ; nos signatures restent en SHA-256.

## Conséquences

- Hors périmètre, documenté :
  - mise en forme complexe (arabe lié, scripts indiens) ;
  - polices CJK ;
  - choix de police au-delà de « DejaVu Sans » ;
  - placement libre du tampon à la souris (le CLI le permet).
- Un document qui utilise du texte Unicode inséré par PDFium grossit d'environ 340 Ko (police
  entière, une fois par fichier).
- Les passes `lopdf` sur un document chiffré déchiffrent en mémoire seulement : le fichier
  écrit est toujours chiffré.
