# ADR-0003 — Double moteur : PDFium principal, `lopdf` pour les passes structurelles

Date : 2026-10-26 · Statut : accepté (en vigueur depuis le Lot 4 ; passes du Lot 5 : ADR-0010)

## Contexte

PDFium couvre rendu, texte, pages, annotations, formulaires et sauvegarde, mais **pas** :
le chiffrement à l'enregistrement, le caviardage fin (il ne supprime que des objets entiers),
l'optimisation (recompression, objets inutilisés), l'écriture des métadonnées, ni certaines
retouches de texte. La fondation doit pourtant accueillir tous les lots.

## Décision

- `mega_core::Document` est **sérialisable en octets** à tout moment (`from_bytes` /
  `to_bytes`) ; PDFium charge ces octets et peut les réécrire.
- À partir du Lot 5, `lopdf` (Rust pur) devient un second moteur pour les **passes
  structurelles** : caviardage, optimisation, chiffrement, métadonnées. Une passe prend les
  octets, produit de nouveaux octets, et le document est rechargé dans PDFium (cache invalidé).
- Les deux moteurs ne travaillent **jamais** simultanément sur la même représentation.
- La modification du texte existant reste « au mieux » (limites du format) ; l'application le
  dit clairement.

## Mise en œuvre effective

- Lot 4 : écriture des valeurs de champs (`forms/write.rs`) — `lopdf` écrit, PDFium recharge
  et régénère les apparences (ADR-0009).
- Lot 5 : passe de nettoyage à l'enregistrement (`cleanup.rs`), filigranes et numéros ajoutés
  comme flux de contenu (`overlay.rs`), lecture d'apparences pour `/InkList` (ADR-0010). Le
  caviardage, lui, est resté côté PDFium (objets entiers) : la passe par caractère dans le flux
  est reportée.
- Invariant respecté partout : `to_bytes()` → `lopdf` → `reload_from_bytes()`, jamais les deux
  moteurs sur la même représentation ; les repères, mesures et titres nécessaires sont
  calculés avec PDFium **avant** la phase `lopdf` (verrou non réentrant).

## Conséquences

- Pas de `lopdf` avant le Lot 4 : le Lot 0 n'ajoute que `pdfium-render` et `image`.
- Pour les tests, les métadonnées sont posées via un squelette PDF écrit à la main
  (`crates/mega-core/src/testing.rs`), première illustration d'une « passe structurelle ».
- Les fixtures chiffrées sont produites hors Rust (`qpdf`) jusqu'au Lot 6.
