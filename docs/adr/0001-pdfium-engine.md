# ADR-0001 — PDFium comme moteur PDF principal

Date : 2026-10-26 · Statut : accepté

## Contexte

Il faut un moteur capable de **rendre** fidèlement n'importe quel PDF et d'**éditer**
(pages, annotations, formulaires, sauvegarde), utilisable depuis Rust, sous licence compatible
avec un projet libre, et suffisamment mature pour ne pas bloquer l'apprentissage.

## Décision

PDFium (moteur de Chrome, licence BSD-3) via la crate `pdfium-render` 0.9.x, avec les
binaires précompilés de `bblanchon/pdfium-binaries`, version épinglée (`chromium/7881`,
celle que cible l'API de `pdfium-render`) et somme de contrôle vérifiée par
`cargo xtask fetch-pdfium`. La bibliothèque n'est pas commitée.

Seul `mega-core` importe `pdfium_render` : les autres crates passent par son API.

## Alternatives écartées

- **MuPDF** : excellent, mais AGPL ; contraignant pour les utilisateurs du projet.
- **`hayro`** (rendu PDF en Rust pur) : prometteur mais encore jeune, lecture seule, pas de
  PDF chiffrés. À réévaluer comme moteur alternatif derrière la frontière `mega-core`.
- **PDF.js côté frontend** : aurait séparé le rendu (JS) de l'édition (Rust) ; un seul moteur
  en Rust garde la cohérence et l'objectif pédagogique.

## Conséquences

- Un composant non-Rust (DLL ≈ 7 Mo) à distribuer à côté de l'exécutable.
- PDFium n'est pas thread-safe : voir ADR-0004.
- Certaines opérations manquent à PDFium (chiffrement à l'écriture, caviardage fin,
  optimisation, métadonnées) : voir ADR-0003.
