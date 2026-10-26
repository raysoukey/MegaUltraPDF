# ADR-0003 — Double moteur : PDFium principal, `lopdf` pour les passes structurelles

Date : 2026-10-26 · Statut : accepté (mise en œuvre de `lopdf` prévue au Lot 5)

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

## Conséquences

- Pas de `lopdf` avant le Lot 5 : le Lot 0 n'ajoute que `pdfium-render` et `image`.
- Pour les tests, les métadonnées sont posées via un squelette PDF écrit à la main
  (`crates/mega-core/src/testing.rs`), première illustration d'une « passe structurelle ».
- Les fixtures chiffrées sont produites hors Rust (`qpdf`) jusqu'au Lot 6.
