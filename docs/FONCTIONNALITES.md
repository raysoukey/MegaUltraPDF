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
- [x] Recherche de texte : surbrillance, suivant/précédent, compteur, sensibilité à la casse [1] ; dans l'ordre de lecture, émojis et caractères hors du plan de base compris [13, 14]
- [x] Propriétés du document (titre, auteur, sujet, mots-clés, dates, producteur, version PDF, nb pages, taille) [1] ; repli sur le XMP quand `/Info` manque, taille physique quand `/UserUnit` ≠ 1 [14]
- [x] Thème clair / sombre, suit le système [1]
- [x] Raccourcis clavier (ouvrir, fermer, zoom, navigation, recherche, rotation) et aide des raccourcis [1]
- [x] Préférences persistantes (zoom par défaut, mode, thème, dossier récent) [1]
- [x] Affichage des pièces jointes du PDF, extraction, ajout, suppression [6] ; y compris celles attachées à une page (annotation `FileAttachment`, avec son numéro de page) [14]

## Pages et documents

- [x] Pivoter des pages (enregistré dans le fichier) [2]
- [x] Supprimer des pages [2]
- [x] Réordonner par glisser-déposer des vignettes [2] ; la page déplacée reste le même objet : signets, liens, balisage et champs qui la visent restent valables [14]
- [x] Extraire des pages vers un nouveau fichier [2]
- [x] Insérer une page vide ; insérer des pages d'un autre PDF [2]
- [x] Fusionner plusieurs fichiers [2]
- [x] Scinder : par plages, toutes les N pages, par signets [2]
- [x] Extraire, fusionner, scinder en gardant le catalogue : formulaire remplissable, signets, destinations, pièces jointes, étiquettes de pages, calques, langue, XMP ; jamais les scripts ni l'arbre de structure (ADR-0019) [14]
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
- [x] Enregistrement conforme : apparences explicites, sous-types natifs pour les marquages (ADR-0008), apparence des marquages écrite même sans affichage préalable [3, 14] ; vérifié par qpdf, et par MuPDF et pdf.js quand ils sont installés (`tests/conformance/`) ; Acrobat non vérifié

## Formulaires et signature

- [x] Afficher et remplir les champs AcroForm (texte, cases, radios, listes et combos ; écriture `lopdf`, apparences régénérées par PDFium) ; XFA détecté [4]
- [x] Navigation au clavier entre champs (contrôles HTML natifs), enregistrement des valeurs [4]
- [x] Aplatir le formulaire [4]
- [x] Signature visuelle : image ou dessin, placement, enregistrement (contenu de page) [4]
- [x] Signature numérique cryptographique (PKCS#12, CMS, mise à jour incrémentale) et vérification des signatures, y compris celles d'autres outils (option `signature`) [8]
- [x] Signature numérique **visible** (tampon : nom, date, motif, image facultative ; pages tournées) ; vérification SHA-384/512 [10]

## Édition de contenu

- [x] Insérer du texte (police, taille, couleur ; Standard 14, WinAnsi) [5]
- [x] Insérer des images [5] ; un JPEG garde ses octets (`/DCTDecode`), masque de transparence seulement si l'image en a [14]
- [x] Déplacer / supprimer des objets de page [5]
- [x] Modifier le texte existant quand c'est possible (même police, même position) [5]
- [x] Caviardage réel (objets texte/image et annotations retirés du fichier, cache noir) [5]
- [x] Caviardage d'une partie d'image (pixels noircis sous la zone, masque compris) et des tracés ; vignettes de page retirées [10]
- [x] Filigrane texte (flux `lopdf`, opacité, inclinaison) ; filigrane image reporté [5]
- [x] Numérotation des pages, en-têtes et pieds de page (modèle `{n}`, `{total}`, `{titre}`) [5]
- [x] Recadrage des pages (marges, `/CropBox`) [5]

## Export, impression, sécurité

- [x] Export en images PNG/JPEG (résolution choisie), export du texte [6]
- [x] Impression (dialogue système, via la vue d'impression du WebView) [6]
- [x] Édition des métadonnées (titre, auteur, sujet, mots-clés) [6] ; reportées dans le XMP existant (`dc:title`, `dc:creator`, `dc:description`, `pdf:Keywords`, créés s'ils manquent ; aucun paquet XMP créé), aucun `/Info` créé sur un PDF/A-4 ; le XMP est conservé à chaque enregistrement, rechiffré dans un document protégé et décompressé quand PDFium l'aurait abîmé (ADR-0019) [14]
- [x] Chiffrement AES-256 / mot de passe et permissions, retrait du mot de passe [6]
- [x] Optimisation : recompression / réduction des images, flux compressés, objets inutilisés [6]
- [x] Comparaison visuelle de deux versions (page par page) [6]

## Finition et distribution

- [x] Interface FR / EN (préférence « système / français / anglais », sans fermer les documents) [7]
- [x] Identité visuelle : icône d'application (fenêtre, installeurs), icônes d'actions sans légende dans la barre d'outils et les menus, logo à l'accueil et dans « À propos » [12]
- [x] Accessibilité : menus au clavier, piège de focus dans les dialogues, noms accessibles, contour de focus, contrastes ≥ 4,5:1 vérifiés par test [7]
- [x] Préférences (langue, thème, zoom, affichage, auteur, position de lecture, récents) [7]
- [x] Aide intégrée (prise en main, raccourcis, « À propos » avec licences tierces) [7]
- [x] Installeurs Windows (NSIS/MSI, vérifié de bout en bout), Linux (AppImage/deb) et macOS (dmg, Apple Silicon) construits par la CI (lancement non vérifié) ; version portable ; PDFium embarqué ; association des `.pdf` ; paquets non signés [7]
- [x] Ouverture d'un PDF depuis le Finder macOS (événement `RunEvent::Opened`) [9] — compilé par la CI macOS, vérification sur un vrai Mac à faire
- [x] Aucun appel réseau, aucune télémétrie (vérifié par test : sources, CSP) [0 → 7]
- [x] OCR par Tesseract en couche de texte invisible, page par page, annulable (option `ocr`) [8]
- [x] OCR de plusieurs pages annulable en un seul pas ; couche invisible en Unicode (grec, cyrillique, CJK recherchables) [10]
- [x] Une seule instance : un PDF ouvert depuis le système arrive en onglet dans la fenêtre existante ; tous les PDF passés en arguments sont ouverts [11]
- [x] Lecteur PDF par défaut : état et bouton dans les préférences (Paramètres Windows, `xdg-mime` sous Linux, marche à suivre macOS) ; enregistrement Windows `Capabilities` [11]
- [x] Texte Unicode (grec, cyrillique, latin étendu) partout où l'application écrit : filigrane, numéros, texte inséré, retouche, texte libre, tampon (police DejaVu Sans incorporée) [10] ; police réduite aux glyphes utilisés dans le fichier enregistré (moins de 40 Kio) [14]
- [x] Documents chiffrés : nettoyage, filigrane, caviardage, formulaires, métadonnées, en gardant la même protection [10]

## Fidélité à l'enregistrement et compatibilité mesurée

Audit `docs/audit/2026-10-31-compatibilite-pdf.md` (137 PDF réels, poppler, MuPDF, pdf.js,
veraPDF) ; décisions : ADR-0019.

- [x] Une suppression (annotation, page, objet, texte remplacé, pièce jointe, caviardage) ne survit jamais dans le fichier : refusée d'emblée si le fichier ne pourra pas être nettoyé (l'ajout reste possible), enregistrement refusé si le nettoyage n'a pas pu s'appliquer [14]
- [x] Version d'en-tête d'origine conservée (`%PDF-2.0` reste 2.0) ; protection AES-256 déclarée (`/Extensions`, en-tête 1.7 au moins) [14]
- [x] Documents PDF/A et PDF/UA : rotation, optimisation, métadonnées, filigrane et numérotation gardent la conformité mesurée par veraPDF ; filigrane, numéros et couche OCR balisés comme artefacts [14]
- [x] Formulaires : `/NeedAppearances` retiré du fichier une fois les apparences régénérées [14]
- [x] Texte extrait : caractères hors du plan de base (émojis, lettres mathématiques), polices CFF CID en OpenType (Noto, Source Han), espaces entre les mots arabes [14]
- [x] Annotations d'autres outils : `FreeText` affichée mais protégée (non éditable), `Popup` masquée de la liste [14]
- [x] Signatures : verdict « document certifié puis modifié » (`/DocMDP` à `P = 1`), horodatage de document reconnu comme tel [14]
- [x] Chaque chemin d'écriture vérifié par qpdf (CI), et par MuPDF, pdf.js et veraPDF quand ils sont installés ; catalogue riche contrôlé après chaque édition (`CONTRIBUTING.md`) [14]

Comportements à connaître (mesurés, non corrigés) :

- **Recherche** : les valeurs des champs de formulaire et le texte des annotations `FreeText` ne
  font pas partie du texte de la page ; une recherche ne les trouve pas (comme pdf.js ; poppler
  et MuPDF les incluent) (L7).
- **Lecture plus tolérante que les autres** : PDFium retrouve un objet absent de la table de
  références mais présent dans le fichier (des signets « ressuscités », par exemple), et
  l'enregistrement l'écrit ; qpdf, MuPDF et pdf.js le traitent comme absent. Le fichier
  enregistré « contient » alors plus que ce qu'un lecteur strict voyait (L9, E15).
- **Chiffrement `Identity` à table abîmée** : un tel fichier révision 6 s'ouvre sans mot de
  passe (comme poppler), là où qpdf, MuPDF et pdf.js en demandent un ; la sortie reproduit
  l'entrée (L10).
- **`/UserUnit`** : la page s'affiche à sa taille nominale (comme PDFium) ; seules les
  Propriétés donnent la taille physique. Le japonais vertical se lit dans un ordre différent de
  poppler, mais cohérent ; un titre en UTF-8 avec marque d'ordre (PDF 2.0) est lu correctement
  (L13).
- **Identifiant `/ID`** : conforme ; un fichier existant garde le premier identifiant et
  renouvelle le second, un document neuf (fusion, extraction, scission) reçoit deux
  identifiants égaux (E12).
- **Flux ajoutés** : les flux écrits par `lopdf` (filigrane, OCR) sont recompressés par PDFium ;
  le fichier final n'a aucun flux sans filtre (E13).
- **Limites de PDFium** : `/ToUnicode` d'une police Type 1 non incorporée ignorée (L4),
  espacement de mots d'une Type 3 non appliqué (L5), glyphes CID sans `/ToUnicode` (L6), JPX à
  transparence intégrée rendu opaque (L8), `Polygon` sans apparence non dessiné (L11).
- **Entrées abîmées** : un défaut de structure de l'original (page sans `/Resources`, arbre de
  noms mal trié, flux suivi de données parasites) est recopié tel quel ; l'enregistrement n'en
  ajoute pas (ADR-0019).

## Qualité transversale (chaque lot)

- [x] Tests automatisés (Rust, TS), CI sur Windows / Linux / macOS [0]
- [x] Relecture indépendante et passe de correction avant fusion [0]
- [x] Note d'apprentissage et fiches concept [0]
- [x] Politique d'erreurs (ADR-0005), aucune panique en production [0]
