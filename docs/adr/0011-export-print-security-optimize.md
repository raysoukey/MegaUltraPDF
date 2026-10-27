# ADR-0011 — Export, impression par le WebView, chiffrement `lopdf`, optimisation conservatrice, comparaison visuelle

Date : 2026-10-27 · Statut : accepté (Lot 6)

## Contexte

Le Lot 6 fait sortir le document (images, texte, papier), le décrit (métadonnées), le
protège (mot de passe, permissions), l'allège (optimisation), gère ses pièces jointes et
compare deux versions. pdfium-render 0.9.4 rend et extrait le texte, lit et écrit les pièces
jointes, mais n'écrit ni métadonnées ni chiffrement, ne connaît pas les révisions de sécurité
5 et 6 (AES-256) et interroge une balise de métadonnée inexistante pour la date de
modification. `lopdf` 0.46 sait chiffrer (V5 / R6), déchiffrer, élaguer, compresser et donne
accès aux XObjects image.

## Décisions

1. **Export** : rendu PDFium à la résolution choisie (36–600 dpi, budget de 50 Mpx vérifié
   **avant** de rendre), PNG sans perte ou JPEG (alpha aplati sur blanc), fichiers
   `<préfixe>-<page>.<ext>` écrits atomiquement et jamais par-dessus un fichier existant sans
   « écraser » ; texte dans l'ordre de lecture de PDFium avec séparateurs facultatifs.
2. **Impression par le WebView** : une vue d'impression HTML (une image par page, servie par
   le protocole `mega://` à 150 ou 300 dpi, `@media print`) et `window.print()` : le dialogue
   d'impression du système, sans code natif ni dépendance par plateforme. Limite : la
   fidélité dépend de l'imprimante virtuelle du WebView (mise à l'échelle « ajuster à la
   page »).
3. **Métadonnées par `lopdf`** : `/Info` (chaînes PDFDoc ou UTF-16BE+BOM, chaîne vide = clé
   retirée, `/ModDate` posé), XMP non touché ; `properties()` lit `/ModDate` lui-même (la
   balise « ModificationDate » demandée par pdfium-render n'existe pas).
4. **Chiffrement AES-256 (V5, révision 6) par `lopdf`**, clé de fichier tirée de `rand`,
   filtres `StdCF`, permissions imprimer / copier / modifier / annoter (les bits voisins —
   haute qualité, accessibilité, assemblage, remplissage — suivent). Le document est
   **rechargé avec son mot de passe** et reste utilisable ; les passes `lopdf` ultérieures le
   refusent (ADR-0010 §7), donc l'interface dit que protéger est la dernière étape. Retrait
   de la protection seulement si le document a été ouvert avec un mot de passe (utilisateur
   ou propriétaire) : un fichier à mot de passe propriétaire seul ouvert sans mot de passe
   ne peut pas prouver le droit de le faire (`WrongPassword`). Les instantanés d'historique
   portent le mot de passe qui relit leurs octets. L'état de sécurité lit `/Encrypt /P`
   lui-même quand PDFium ne connaît pas la révision.
5. **Optimisation conservatrice** : élagage, Flate sur les flux non compressés, recompression
   JPEG et réduction des seules images entièrement comprises (DeviceRGB / DeviceGray 8 bits,
   sans masque, `/Decode`, `/ImageMask`, prédicteur), remplacement seulement si plus petit,
   document inchangé si le fichier entier ne rétrécit pas, réécriture validée
   (`rewrite_is_sane`). Le rapport dit ce qui a été fait.
6. **Pièces jointes par PDFium** : liste dans l'ordre de l'arbre de noms, extraction vers un
   chemin choisi par l'utilisateur (jamais composé à partir du nom déclaré), ajout borné à
   100 Mo avec dédoublonnage du nom.
7. **Comparaison visuelle** : rendu des deux pages à la même échelle (≤ 4), pixels identiques
   (tolérance 24 par canal) estompés, différents en rouge, union des tailles ; l'image PNG et
   les compteurs traversent la frontière Tauri dans un tampon brut (`ipc::Response`, 16 octets
   d'en-tête) plutôt qu'en JSON base64.
8. Toute édition reste une `EditCommand` (`SetMetadata`, `Protect`, `Unprotect { password }`,
   `Optimize`, `AddAttachment`, `DeleteAttachment`) ; la CLI écrit par `save_as`.
9. **Après relecture** (code + conformité PDF) :
   - protéger applique d'abord la passe d'enregistrement (`/InkList`, clé privée) : ensuite elle
     s'abstient sur un fichier chiffré ;
   - le bit 10 (accessibilité) reste toujours à 1, `/Encrypt` porte `/Length 256` et `/StdCF`
     `/Length 32 /AuthEvent /DocOpen` (`qpdf --check` propre) ;
   - mots de passe limités à ASCII imprimable et lettres Latin-1 (stables par SASLprep, que lopdf
     applique et PDFium non) ;
   - permissions d'un fichier à mot de passe propriétaire seul lues dans l'`encryption_state` de
     lopdf (qui retire `/Encrypt` en déchiffrant) ; déprotection avec un mot de passe **saisi**,
     vérifié par PDFium (lopdf ouvre un tel fichier avec n'importe quel mot de passe) ;
   - l'optimisation se prévisualise (`optimize_preview`, sur une copie) ; sans gain, la commande
     est refusée (pas d'étape vide dans l'historique) ; masques `/SMask` / `/Mask`, JPEG à
     `/DecodeParms` et flux XMP `/Metadata` laissés intacts ;
   - comparaison : l'union des tailles est bornée (50 Mpx) ; préfixe d'export sans `: * ? " < > |` ;
     nom de pièce jointe réduit à un nom de fichier sûr (dernier composant, sans contrôle ni bidi) ;
   - impression : `@media print` masque toute l'application et lève la mise en page plein écran.
   Décisions assumées : retrait de protection possible avec le seul mot de passe d'ouverture ;
   permissions de l'auteur non appliquées à l'export ; pas de `/Extensions /ADBE`.

## Conséquences

- `rand` devient une dépendance directe de `mega-core` (déjà dans l'arbre via lopdf).
- Pièges consignés dans CLAUDE.md : révisions de sécurité inconnues de pdfium-render, balise
  « ModificationDate », ordre des pièces jointes, mots de passe limités à Latin-1 (SASLprep
  hors périmètre).
- Dettes : export PDF/A, comparaison textuelle, impression silencieuse, recompression des
  images CMYK / indexées / masquées, filigrane image, XMP.
