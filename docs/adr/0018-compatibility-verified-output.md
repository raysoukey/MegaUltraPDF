# ADR-0018 — Compatibilité : sorties vérifiées, chiffrements, textes non latins, PDF/A

Date : 2026-10-30 · Statut : accepté (Lot 13, PR B)

## Contexte

L'audit du Lot 13 (`docs/audit/2026-10-30-robustesse-compatibilite.md`, C1 à C10) a relevé
plusieurs manques :
- rien ne vérifiait que nos fichiers s'ouvrent dans d'autres lecteurs ;
- la signature de fichiers à structure inhabituelle n'était pas testée ;
- plusieurs chiffrements n'avaient jamais été essayés ;
- l'hébreu, l'arabe et l'écriture verticale étaient lus dans le désordre ;
- la recherche ne suivait pas le même ordre que la sélection ;
- une déclaration PDF/A n'était ni signalée ni protégée.

## Décisions

1. **qpdf comme juge indépendant** (`tests/conformance.rs`, devenu le dossier
   `tests/conformance/` au Lot 14).
   - Chaque variante d'`EditCommand` est écrite en JSON, comme l'interface l'envoie, puis
     appliquée et enregistrée.
   - S'y ajoutent la couche OCR, la signature numérique et la réécriture des fichiers abîmés du
     corpus.
   - Chaque sortie passe `qpdf --check`, sans erreur **ni** avertissement.
   - Un test garde-fou prouve que le contrôle rejette un fichier abîmé.
   - La CI installe qpdf sur les trois systèmes et pose `MEGA_REQUIRE_QPDF=1`. En local, sans
     qpdf, ces tests se signalent et passent.
2. **Signer ce que lopdf voit comme PDFium**. Avant d'ajouter une mise à jour incrémentale,
   le fichier est réécrit dans trois cas :
   - lopdf n'y voit pas le même nombre de pages que PDFium, ou pas de `/Root` ;
   - il a dû reconstruire la table de références ;
   - des octets précèdent `%PDF-`.

   Les tests de conformité ont trouvé deux vrais défauts :
   - une table `xref` décalée se lisait « sans erreur » mais sans aucune page, et la
     signature échouait ;
   - un préfixe parasite faisait écrire des positions fausses, et qpdf jugeait le fichier
     abîmé.
3. **Chiffrements** (`fixtures/chiffrement/`, produits par qpdf) :
   - RC4 40 bits, AES-128 avec deux mots de passe distincts, AES-256 révision 5, révision 6 avec
     un mot de passe Unicode ;
   - tous s'ouvrent, se modifient et s'enregistrent en gardant leur protection.

   **Mots de passe** : est accepté tout mot de passe que SASLprep laisse inchangé, quelle que
   soit son écriture (la limite était U+00FF). lopdf applique SASLprep en chiffrant, PDFium ne
   l'applique pas en ouvrant : un mot de passe que SASLprep réécrirait est donc refusé à la
   protection. Le message ne cite jamais le caractère. `stringprep` devient une dépendance
   directe ; il était déjà dans l'arbre.

   **Chiffrement par certificat** (`/Adobe.PubSec`) : `CoreError::UnsupportedEncryption`, au
   lieu d'une erreur brute de PDFium suivie d'une demande de mot de passe sans issue.
4. **Droite à gauche** : `reading_order::layout_chars` remet dans l'ordre logique toute ligne
   dont les lettres sont surtout hébraïques ou arabes. Ses suites latines ou numériques restent
   de gauche à droite (version simplifiée de UAX #9). L'export, la copie et la recherche
   mesurent les blancs dans les deux sens.
5. **Écriture verticale** : `reading_order::vertical_runs` repère les caractères CJK dessinés
   d'affilée et empilés. Ils sont lus en un bloc, de haut en bas, comme le texte tourné.
6. **Recherche = ordre de lecture** :
   - `search_page` cherche dans les `text_boxes`, et non plus avec la recherche de PDFium
     (ordre de dessin) ;
   - un changement de ligne ou un blanc entre un libellé et sa valeur compte comme une espace ;
   - les rectangles sont fusionnés par ligne.
7. **PDF/A** :
   - `DocumentProperties::pdfa` lit la déclaration (`pdfaid:part`, `pdfaid:conformance`) dans
     le XMP ;
   - la déclaration, `/Metadata` et `/OutputIntents` sont conservés (test) ;
   - les Propriétés l'affichent ;
   - enregistrer un PDF/A modifié prévient que la conformité n'est plus garantie.

   On ne retire pas la déclaration, car la plupart des éditions la respectent. On ne la
   vérifie pas non plus, ce qui serait le travail d'un validateur comme veraPDF.

   **Mise à jour (Lot 14, 2026-10-31)** : mesures veraPDF de l'audit
   `docs/audit/2026-10-31-compatibilite-pdf.md` (constats E2 et E6), sur les 16 fichiers PDF/A
   et 7 PDF/UA du corpus.
   - **Avant le lot** : rotation et optimisation gardaient PDF/A-1b, 2b, 2u, 3b et PDF/UA-1 ;
     le filigrane cassait 11 fichiers sur 16 ; les métadonnées cassaient PDF/A-1b quand le XMP
     n'avait pas de `dc:title` ; **toute** édition cassait PDF/A-4 et PDF/UA-2 (en-tête
     réécrit `%PDF-1.0`).
   - **Corrections** : en-tête d'origine reposé (ADR-0019 §3) ; filigrane, numéros et couche
     OCR balisés comme artefacts, police incorporée, couleur `CalRGB` en PDF/A, transparence
     simulée en PDF/A-1, groupe de transparence de page ailleurs (ADR-0010, mise à jour) ;
     `/CIDSet` dans les sous-ensembles de la police Unicode ; titre, auteur, sujet et
     mots-clés écrits dans le XMP ; aucun `/Info` créé sur un PDF/A-4 ; `/NeedAppearances`
     retiré et apparences des marquages écrites (ADR-0008 et ADR-0009, mises à jour).
   - **Après le lot** (banc d'écriture de l'audit, rejoué) : rotation, optimisation et
     métadonnées gardent toutes les parties, PDF/A-4 et PDF/UA-2 comprises ; filigrane et
     numéros gardent la conformité des 23 fichiers. Des tests le vérifient quand
     `MEGA_VERAPDF` et `MEGA_CORPUS` sont posés : filigrane et numéros sur tout le corpus et
     nouveau titre d'un PDF/A-4 (`tests/conformance/corpus.rs`), métadonnées d'un PDF/A-1b
     (`tests/metadata.rs`).
   - **Non mesuré** : les autres éditions (texte inséré en police Standard 14 non incorporée,
     annotations, images, formulaires). L'avertissement à l'enregistrement reste la réponse.

## Limites assumées

- **Glyphes sans Unicode** (police CID sans `/ToUnicode`) : ils gardent leur place, mais PDFium
  prend leur code comme caractère, ce qui donne du charabia à la copie. Les signaler
  demanderait `FPDFText_HasUnicodeMapError`, que pdfium-render 0.9.4 n'expose pas ; l'appeler
  soi-même exigerait du code `unsafe`.
- **Mise en page de droite à gauche** : les colonnes d'une page en arabe ou en hébreu se lisent
  encore de gauche à droite (la colonne de gauche d'abord). Le texte inséré par l'utilisateur
  reste refusé pour ces écritures (`fonts::check_drawable`, pas de mise en forme
  contextuelle).
- **PDF/A** : aucune validation de conformité dans l'application ; veraPDF n'est employé que
  par les tests (Lot 14).

## Mise à jour (Lot 14, 2026-10-31) : texte extrait

Constats L1 à L3 de l'audit `docs/audit/2026-10-31-compatibilite-pdf.md`.

- **Caractères hors du plan de base** (L1 : émojis, lettres mathématiques, CJK étendu) : PDFium
  les rend en deux moitiés UTF-16 ; `text::drawn_chars` les réunit en un seul caractère, dont
  le rectangle couvre les deux. Ils entrent dans le texte, la recherche et la copie.
- **Police CFF CID en OpenType** (L2, polices Noto ou Source Han) : PDFium dessine la page mais
  n'expose aucun caractère. Quand une page porte du texte et que PDFium n'en donne aucun,
  `text/tounicode.rs` relit son contenu avec `lopdf` et traduit chaque code par la
  `/ToUnicode` de sa police. C'est un repli borné, pas un second moteur de texte : une police
  sans `/ToUnicode` n'y donne rien, et l'écriture verticale y est placée comme l'horizontale.
- **Mots arabes collés** (L3) : dans une ligne surtout écrite de droite à gauche et sans aucune
  espace, un écart de plus d'un quart de cadratin entre deux caractères reçoit une espace
  (`reading_order::word_gaps`). PDFium le fait déjà pour l'écriture de gauche à droite.
- Restent des limites de PDFium (L4 à L6) : `/ToUnicode` d'une Type 1 non incorporée ignorée,
  espacement de mots d'une Type 3 non appliqué, glyphes CID sans `/ToUnicode`.
