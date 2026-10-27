# Fonctionnalités de référence d'un logiciel PDF

Liste de contrôle qui définit « 100 % fonctionnel » pour MegaUltraPDF. Chaque ligne est cochée
quand elle est livrée, testée et documentée. Le lot cible est entre crochets ; la feuille de
route (`ROADMAP.md`) donne l'ordre, la conception (`superpowers/specs/`) le détail.

## Lecture

- [x] Ouvrir un PDF depuis un dialogue [0]
- [x] Ouvrir un PDF passé en argument au lancement [0]
- [x] Ouvrir par glisser-déposer sur la fenêtre [1]
- [x] Fichiers récents (menu, persistants) [1]
- [x] Documents protégés : saisie du mot de passe, distinction absent/incorrect [1]
- [x] Plusieurs documents ouverts en onglets ; fermer un onglet [1]
- [x] Rendu fidèle d'une page (PDFium) [0]
- [x] Rendu net à tout zoom, rerendu après zoom, placeholder pendant le rendu [1]
- [x] Rendu asynchrone (acteur PDFium), pages visibles en priorité, cache LRU [1]
- [x] Zoom : %, ajuster à la largeur, ajuster à la page, molette + Ctrl, raccourcis [1]
- [x] Modes : page unique, défilement continu [1]
- [x] Rotation d'affichage (90°) [1]
- [x] Navigation : page précédente/suivante, première/dernière, saisie du numéro, position persistante par document [1]
- [x] Vignettes des pages (panneau latéral) [1]
- [x] Signets / table des matières (outline) et saut à la destination [1]
- [x] Liens internes cliquables ; liens externes ouverts dans le navigateur après confirmation [1]
- [x] Calque de texte : sélection à la souris, copie dans le presse-papiers [1]
- [x] Recherche de texte : surbrillance, suivant/précédent, compteur, sensibilité à la casse [1]
- [x] Propriétés du document (titre, auteur, sujet, mots-clés, dates, producteur, version PDF, nb pages, taille) [1]
- [x] Thème clair / sombre, suit le système [1]
- [x] Raccourcis clavier (ouvrir, fermer, zoom, navigation, recherche, rotation) et aide des raccourcis [1]
- [x] Préférences persistantes (zoom par défaut, mode, thème, dossier récent) [1]
- [x] Affichage des pièces jointes du PDF, extraction, ajout, suppression [6]

## Pages et documents

- [x] Pivoter des pages (enregistré dans le fichier) [2]
- [x] Supprimer des pages [2]
- [x] Réordonner par glisser-déposer des vignettes [2]
- [x] Extraire des pages vers un nouveau fichier [2]
- [x] Insérer une page vide ; insérer des pages d'un autre PDF [2]
- [x] Fusionner plusieurs fichiers [2]
- [x] Scinder : par plages, toutes les N pages, par signets [2]
- [x] Enregistrer / Enregistrer sous (écriture atomique), indicateur « modifié », confirmation à la fermeture [2]
- [x] Annuler / Rétablir pour toute édition [2]
- [x] CLI : `merge`, `split`, `rotate`, `extract`, `info`, `render` [0, 2]

## Annotations

- [x] Surligner, souligner, barrer du texte [3]
- [x] Note (commentaire), texte libre [3]
- [x] Encre (dessin à main levée) [3]
- [x] Formes : rectangle, ellipse, ligne, flèche [3]
- [x] Tampons prédéfinis [3]
- [x] Propriétés : couleur, opacité, épaisseur, auteur, date [3]
- [x] Déplacer, redimensionner, supprimer ; panneau liste des annotations [3]
- [x] Enregistrement conforme : apparences explicites, sous-types natifs pour les marquages (ADR-0008) ; vérifié par relecture PDFium et revue `pdf-expert`, à confirmer manuellement dans Acrobat / PDF.js [3]

## Formulaires et signature

- [x] Afficher et remplir les champs AcroForm (texte, cases, radios, listes et combos ; écriture `lopdf`, apparences régénérées par PDFium) ; XFA détecté [4]
- [x] Navigation au clavier entre champs (contrôles HTML natifs), enregistrement des valeurs [4]
- [x] Aplatir le formulaire [4]
- [x] Signature visuelle : image ou dessin, placement, enregistrement (contenu de page) [4]
- [x] Signature numérique cryptographique (PKCS#12, CMS, mise à jour incrémentale) et vérification des signatures, y compris celles d'autres outils (option `signature`) [8]

## Édition de contenu

- [x] Insérer du texte (police, taille, couleur ; Standard 14, WinAnsi) [5]
- [x] Insérer des images [5]
- [x] Déplacer / supprimer des objets de page [5]
- [x] Modifier le texte existant quand c'est possible (même police, même position) [5]
- [x] Caviardage réel (objets texte/image et annotations retirés du fichier, cache noir) [5]
- [x] Filigrane texte (flux `lopdf`, opacité, inclinaison) ; filigrane image reporté [5]
- [x] Numérotation des pages, en-têtes et pieds de page (modèle `{n}`, `{total}`, `{titre}`) [5]
- [x] Recadrage des pages (marges, `/CropBox`) [5]

## Export, impression, sécurité

- [x] Export en images PNG/JPEG (résolution choisie), export du texte [6]
- [x] Impression (dialogue système, via la vue d'impression du WebView) [6]
- [x] Édition des métadonnées (titre, auteur, sujet, mots-clés ; XMP non touché) [6]
- [x] Chiffrement AES-256 / mot de passe et permissions, retrait du mot de passe [6]
- [x] Optimisation : recompression / réduction des images, flux compressés, objets inutilisés [6]
- [x] Comparaison visuelle de deux versions (page par page) [6]

## Finition et distribution

- [x] Interface FR / EN (préférence « système / français / anglais », sans fermer les documents) [7]
- [x] Accessibilité : menus au clavier, piège de focus dans les dialogues, noms accessibles, contour de focus, contrastes ≥ 4,5:1 vérifiés par test [7]
- [x] Préférences (langue, thème, zoom, affichage, auteur, position de lecture, récents) [7]
- [x] Aide intégrée (prise en main, raccourcis, « À propos » avec licences tierces) [7]
- [x] Installeurs Windows (NSIS/MSI, vérifié de bout en bout), Linux (AppImage/deb) et macOS (dmg, Apple Silicon) construits par la CI (lancement non vérifié) ; version portable ; PDFium embarqué ; association des `.pdf` ; paquets non signés [7]
- [ ] Ouverture d'un PDF depuis le Finder macOS (événement Apple) [consolidation]
- [x] Aucun appel réseau, aucune télémétrie (vérifié par test : sources, CSP) [0 → 7]
- [x] OCR par Tesseract en couche de texte invisible, page par page, annulable (option `ocr`) [8]

## Qualité transversale (chaque lot)

- [x] Tests automatisés (Rust, TS), CI sur Windows / Linux / macOS [0]
- [x] Relecture indépendante et passe de correction avant fusion [0]
- [x] Note d'apprentissage et fiches concept [0]
- [x] Politique d'erreurs (ADR-0005), aucune panique en production [0]
