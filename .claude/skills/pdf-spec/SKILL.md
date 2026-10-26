---
name: pdf-spec
description: Rappel de ce que dit la norme PDF (ISO 32000-1/2) sur un sujet précis (annotations, AcroForm, chiffrement, xref, chaînes de texte, structure des pages…) avant d'implémenter une fonctionnalité, avec les fixtures à prévoir et les points à vérifier dans pdfium-render. Utiliser quand l'utilisateur dit « /pdf-spec <sujet> » ou avant toute tâche qui écrit dans un PDF.
---

# Rappel de la norme PDF

Sujet : `$ARGUMENTS`.

Produire, en français, une fiche courte (≤ 100 lignes) :

1. **Ce que dit la norme** (ISO 32000-2:2020, et différences notables avec 32000-1/PDF 1.7) :
   les objets et dictionnaires concernés, leurs clés obligatoires et optionnelles, les
   valeurs admises, le numéro de section. S'appuyer sur la connaissance interne ; si un
   point est incertain, le dire et proposer une recherche web ciblée.
2. **Ce que font les autres lecteurs** (Acrobat, PDF.js, SumatraPDF, Aperçu) quand la
   structure est incomplète ou non conforme : c'est le critère « lisible ailleurs » du projet.
3. **Ce que PDFium / `pdfium-render` expose** pour ce sujet : noms des types et méthodes
   (vérifier dans `~/.cargo/registry/src/*/pdfium-render-*/src/` plutôt que de mémoire),
   limites connues (ex. : pas de chiffrement à l'écriture, pas de setter de métadonnées).
4. **Fixtures à prévoir** dans `fixtures/` : quels fichiers, comment les produire
   (`cargo xtask gen-fixtures`, `qpdf`, squelette PDF écrit à la main comme dans
   `crates/mega-core/src/testing.rs`), et ce que chaque test doit vérifier.
5. **Pièges d'encodage** : chaînes de texte (PDFDocEncoding vs UTF-16BE+BOM), dates
   (`D:YYYYMMDDHHmmSS`), rectangles (coordonnées PDF, origine en bas à gauche), unités (points).

Terminer par la liste des décisions à prendre avant de coder, formulées en questions fermées.
