# Feuille de route

Les fonctionnalités sont livrées par lots. Chaque lot a sa spec et son plan dans
`docs/superpowers/`, et sa note d'apprentissage dans `docs/apprentissage/`.
Légende : ✅ terminé · 🚧 en cours · ⬜ à venir. Démarrer un lot : `/lot N`.

| Lot | Nom | Contenu | État |
|---|---|---|---|
| 0 | Fondations et environnement | Outillage, workspace Cargo, `xtask fetch-pdfium`, `mega-core` (ouverture, métadonnées, rendu), `mega-cli info/render`, coquille Tauri 2 + Svelte affichant la page 1, environnement Claude Code, CI | ✅ |
| 1 | Lecteur | Ouverture (dialogue, glisser-déposer, argument, récents), mot de passe, rendu net à tout zoom avec cache et acteur PDFium, modes page/continu, navigation, vignettes, signets, recherche, calque de texte (sélection, copie), thème, raccourcis, onglets | ✅ |
| 2 | Pages et documents | Pivoter, supprimer, réordonner, extraire, insérer, fusionner, scinder, enregistrer (écriture atomique), annuler/rétablir (patron Command), CLI `merge/split/rotate/extract` | ✅ |
| 3 | Annotations | Surligner, souligner, barrer, note, texte libre, encre, formes, tampon ; propriétés ; panneau des annotations ; calque SVG | ✅ |
| 4 | Formulaires et signature visuelle | Champs AcroForm en champs HTML, remplissage, enregistrement, aplatir ; image ou dessin de signature | ✅ |
| 5 | Édition de contenu | Insérer texte et images, déplacer/supprimer des objets, retouche de texte « au mieux », caviardage réel (première passe `lopdf`), filigrane, numérotation, recadrage ; dettes `lopdf` du Lot 3 (`/InkList`, `/AP` des notes, élagage), pages tournées | ✅ |
| 6 | Export, impression, sécurité | Export images/texte, impression via le WebView, métadonnées, chiffrement (`lopdf`), optimisation | ✅ |
| 7 | Finition et distribution | i18n FR/EN, accessibilité, préférences, aide, installeurs (bundler Tauri), options désactivées par défaut (OCR, signature cryptographique) | ⬜ |

Référence complète : `docs/superpowers/specs/2026-10-26-megaultrapdf-design.md`, §5.
