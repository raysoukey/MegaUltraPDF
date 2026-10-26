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
- [ ] Affichage des pièces jointes du PDF et extraction [6]

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

- [ ] Surligner, souligner, barrer du texte [3]
- [ ] Note (commentaire), texte libre [3]
- [ ] Encre (dessin à main levée) [3]
- [ ] Formes : rectangle, ellipse, ligne, flèche [3]
- [ ] Tampons prédéfinis [3]
- [ ] Propriétés : couleur, opacité, épaisseur, auteur, date [3]
- [ ] Déplacer, redimensionner, supprimer ; panneau liste des annotations [3]
- [ ] Enregistrement conforme (lisible dans Acrobat, PDF.js, SumatraPDF, Aperçu) [3]

## Formulaires et signature

- [ ] Afficher et remplir les champs AcroForm (texte, cases, radios, listes, dates) [4]
- [ ] Navigation au clavier entre champs, enregistrement des valeurs [4]
- [ ] Aplatir le formulaire [4]
- [ ] Signature visuelle : image ou dessin, placement, enregistrement [4]
- [ ] Signature numérique cryptographique (option désactivée par défaut) [7]

## Édition de contenu

- [ ] Insérer du texte (police, taille, couleur) [5]
- [ ] Insérer des images [5]
- [ ] Déplacer / supprimer des objets de page [5]
- [ ] Modifier le texte existant quand c'est possible, avec avertissement sur les limites [5]
- [ ] Caviardage réel (contenu supprimé) [5]
- [ ] Filigrane texte ou image [5]
- [ ] Numérotation des pages, en-têtes et pieds de page [5]
- [ ] Recadrage des pages [5]

## Export, impression, sécurité

- [ ] Export en images PNG/JPEG (résolution choisie), export du texte [6]
- [ ] Impression (dialogue système) [6]
- [ ] Édition des métadonnées [6]
- [ ] Chiffrement / mot de passe, retrait du mot de passe [6]
- [ ] Optimisation : recompression des images, objets inutilisés [6]
- [ ] Comparaison de deux versions [6]

## Finition et distribution

- [ ] Interface FR / EN [7]
- [ ] Accessibilité : clavier complet, lecteurs d'écran, contrastes [7]
- [ ] Aide intégrée [7]
- [ ] Installeurs Windows (NSIS/MSI), Linux (AppImage/deb), macOS (dmg) ; version portable ; DLL PDFium embarquée [7]
- [ ] OCR (option désactivée par défaut) [7]
- [ ] Aucun appel réseau, aucune télémétrie (vérifié) [0 → 7]

## Qualité transversale (chaque lot)

- [x] Tests automatisés (Rust, TS), CI sur Windows / Linux / macOS [0]
- [x] Relecture indépendante et passe de correction avant fusion [0]
- [x] Note d'apprentissage et fiches concept [0]
- [x] Politique d'erreurs (ADR-0005), aucune panique en production [0]
