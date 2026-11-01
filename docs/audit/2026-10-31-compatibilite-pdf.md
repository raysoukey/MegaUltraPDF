# Audit de compatibilité PDF en lecture et en écriture (2026-10-31)

Audit **mesuré** de la version 0.13.0 (branche `audit-compatibilite`), sans correction de code :
ce que MegaUltraPDF lit et écrit correctement, et ce qui ne l'est pas, confronté à de vrais PDF et
à quatre moteurs indépendants. Chaque constat renvoie à une mesure rejouable (annexe) ou à un
`fichier:ligne`. Les corrections proposées forment le **Lot 14**, à valider avant tout code.

Gravité : **É** élevée (perte ou fuite de données, fichier faux ou illisible ailleurs), **M**
moyenne (fonction fausse sur une classe de fichiers réels), **F** faible (cas rare, taille,
conformité d'une norme que l'on ne promet pas), **I** informatif (comportement à connaître, pas
de correction).

## 1. Méthode

**Outils et versions** (poste Windows 11, installés en développement seulement, jamais dans
l'application) : PDFium `chromium/7881` via pdfium-render 0.9.4 et lopdf 0.46 (les nôtres) ;
qpdf 12.4.2 ; poppler 25.07.0 (`pdfinfo`, `pdftotext`, `pdftoppm`, `pdffonts`) ; MuPDF 1.27.0
(`mutool`) ; pdf.js 5.4.149 sous Node 22 (moteur de Firefox) ; veraPDF 1.30.3 (validateur
PDF/A et PDF/UA de référence) ; Tesseract 5.4.0 ; Edge et Chrome 156 (impression PDF). Chemins
dans `scripts/audit/*.sh` (variables `MEGA_QPDF`, `MEGA_MUTOOL`, `MEGA_POPPLER_BIN`,
`MEGA_PDFJS_CHECK`, `MEGA_VERAPDF`).

**Corpus réel** (`fixtures/corpus/`, ignoré par Git, manifeste et sommes SHA-256 dans
`fixtures/corpus/MANIFEST.md`, récupération par `scripts/audit/fetch-corpus.sh`) :

| Famille | Fichiers | Provenance |
|---|---|---|
| producteurs (pdfTeX, Word, OpenOffice, Acrobat…) | 10 | suites pdf.js et qpdf |
| formulaires (IRS f1040, 160F, widgets, XFA) | 11 | pdf.js, qpdf |
| annotations tierces (tous sous-types, avec et sans `/AP`) | 14 | pdf.js |
| images (JPEG CMJN, CCITT, JBIG2, JPX, scans, DeviceN) | 14 | pdf.js, qpdf |
| polices (CID TrueType arabe, CFF, Type1, Type3, vertical, CJK) | 18 | pdf.js |
| structure (balisage, calques, XObjects) | 6 | pdf.js |
| navigation (signets, destinations, étiquettes, pièces jointes, JavaScript) | 13 | pdf.js, qpdf |
| fichier (linéarisé, flux d'objets, révisions, xref abîmées) | 14 | qpdf, pdf.js |
| chiffrement (RC4-40, AES-256 R6, pièces jointes chiffrées) | 8 | qpdf |
| signatures | 1 | pdf.js |
| pdfa (PDF/A-1b, 2b, 2u, 3b, 4, fichiers `pass`) | 16 | corpus veraPDF |
| pdfua (PDF/UA-1, UA-2, fichiers `pass`) | 7 | corpus veraPDF |
| local (Edge, Chrome, Tesseract) | 5 | générés sur ce poste |
| **total** | **137** | |

**Trois bancs** :

1. `scripts/audit/read-bench.sh` → `target/audit/lecture.csv` : pour chaque fichier, `mega info`,
   texte de la page 1 et rendu à 72 dpi, comparés à poppler, MuPDF et pdf.js (nombre de pages,
   similarité des mots par indice de Jaccard, pourcentage de pixels différents après passage en gris).
2. `scripts/audit/write-bench.sh` → `target/audit/ecriture.csv` : quatre éditions par le CLI
   (rotation d'une page, filigrane Unicode « AUDIT — été 2026 Ω », métadonnées accentuées,
   optimisation), chaque sortie jugée par `qpdf --check`, `mutool draw` (alertes), pdf.js
   (ouverture), poppler (mots de la page 1 retrouvés), clés du catalogue, signets, pièces jointes,
   AcroForm avant et après ; puis veraPDF avant et après sur les familles `pdfa` et `pdfua`.
3. `crates/mega-core/tests/audit_compat.rs` (20 expériences `#[ignore]`, verdicts dans
   `target/audit/audit_compat.txt`) : PDF synthétiques qui isolent un point (catalogue riche de 17
   structures, repli du nettoyage, formulaires, chiffrement, images, polices, annotations tierces,
   arbre de pages profond, destinations nommées, métadonnées, signatures, les écarts du corpus, et cinq
   remesures demandées par la relecture normative de l'agent `pdf-expert`, §4 bis).
   **Retiré au Lot 14** (tâche 23) : chaque expérience est devenue un test qui affirme
   (correspondance au §9) ;
   il se relit et se relance dans un arbre de travail à la révision `e2d8338`.

**Non vérifié** : Acrobat, Aperçu et Edge en tant que lecteurs graphiques (choix du
propriétaire : outils en ligne de commande seulement) ; export Word par COM (refusé par le
garde-fou de l'environnement, script `scripts/audit/gen-word.ps1` prêt) ; horodatage
`ETSI.RFC3161` réel ; second repli de `before_save` (`cleanup.rs:57`, relecture divergente, non
forçable).

## 2. Ce qui est solide (mesuré)

- **Ouverture** : 133 fichiers réels sur 137 s'ouvrent ; les 4 refus sont justes (3 fichiers
  chiffrés sans mot de passe connu → « protégé par un mot de passe », 1 fichier `bad-content.pdf`
  refusé par les quatre moteurs). **Aucune panique**, aucun écart de nombre de pages avec poppler et
  pdf.js (MuPDF, lui, échoue sur `append-xref-loop.pdf`).
- **Fichiers abîmés** (xref décalée ou absente, `endobj` manquant, LZW cassé, préfixe parasite,
  fuzzé) : lus comme poppler et pdf.js, réécrits sans nouvelle alerte qpdf.
- **Chiffrement** : RC4-40 R3, AES-128, AES-256 R5 et R6, mot de passe utilisateur ou
  propriétaire, pièces jointes chiffrées seules, métadonnées en clair : lecture, texte et rendu
  identiques aux trois moteurs ; réécriture rechiffrée à l'identique, acceptée par qpdf, MuPDF et
  pdf.js avec le même mot de passe.
- **Rendu** : identique (< 5 % de pixels) à au moins deux moteurs sur 129 fichiers sur 133, y
  compris CCITT, JBIG2, JPX, JPEG CMJN, DeviceN, transparence, Type3, polices non incorporées.
- **Texte** : identique (≥ 80 % de mots communs) à au moins deux moteurs sur 118 fichiers sur
  133 ; les 15 écarts sont expliqués en §3 (6 constats, 9 artefacts des moteurs de référence).
- **Écriture** sur le corpus : 522 éditions réussies sur 532 ; les 10 refus sont volontaires et
  bien libellés (optimisation d'un PDF chiffré, réécriture d'un chiffrement par filtre `Identity`).
  **Toutes** les sorties passent `qpdf --check` sans alerte nouvelle (9 fichiers gardent les alertes
  qu'ils avaient en entrée, jamais plus), **0 alerte MuPDF** sur 522 sorties, pdf.js ouvre 521
  sorties (la 522ᵉ était déjà fermée à pdf.js en entrée). Aucune clé du catalogue perdue,
  signets, pièces jointes et AcroForm inchangés, texte de la page 1 intégralement retrouvé par
  poppler (sauf deux fichiers dont poppler change lui-même l'ordre des mots après rotation).
- **Banc synthétique** : sur un catalogue riche de 17 structures (balisage, calques, destinations
  nommées, étiquettes de pages, JavaScript, pièces jointes, XMP, OutputIntents, signets, liens,
  AcroForm), `rotate_pages` et `delete_pages` ne perdent **rien** ; l'arbre de pages de 50 niveaux
  avec attributs hérités est lu et réécrit correctement ; les destinations nommées (`/Names /Dests`
  et `/Dests` 1.1) sont résolues dans signets et liens ; les sorties de toutes les commandes non
  couvertes par `conformance.rs` (E9 : `unprotect`, cases, radio, combo, ellipse, flèche, tampon
  Unicode, filigrane et numéros Unicode, texte Helvetica, image JPEG, éditions sur fichier chiffré,
  optimisation avec images, signature visible, `merge`, `split`, `extract`) passent qpdf, MuPDF et
  pdf.js ; 39 sorties sur 39 sans alerte MuPDF, 37 sur 37 ouvertes par pdf.js.

## 3. Lecture : constats

| # | G | Constat | Preuve | Proposition |
|---|---|---|---|---|
| L1 | É | **Caractères hors du plan de base perdus** (émojis, alphabets mathématiques `𝜋 𝑛` de Cambria Math, CJK étendu) : absents du texte, de la recherche et de la copie, alors que les trois moteurs les extraient | corpus `bug1937438_from_word.pdf` (Word) : 60 boîtes, 0 hors BMP, pdf.js en voit 4 ; `local/edge-article.pdf` : `😀` absent ; cause : `text.rs:118` écarte tout caractère dont `unicode_char()` est `None`, ce qui est le cas des deux moitiés de paire de substitution que PDFium renvoie (UTF-16) | apparier les moitiés `D800–DBFF` + `DC00–DFFF` via `unicode_value()` avant de bâtir le `DrawnChar` |
| L2 | É | **Police CFF CID en OpenType (`/FontFile3 /Subtype /OpenType`, Noto Sans TC) : aucun texte extrait** (0 boîte), alors que la page se rend et que les trois moteurs lisent « 除 » ; touche les PDF chinois, japonais et coréens produits avec les polices Noto/Source Han | corpus `polices/Embedded_font.pdf` ; `[L19-Embedded_font.pdf] boîtes PDFium => 0` ; rendu identique à MuPDF | isoler dans PDFium (chars sans valeur Unicode ? `loose_bounds` en erreur ?) par une fixture minimale ; si PDFium ne lit pas la police, repli : lire `/ToUnicode` nous-mêmes par lopdf pour la page |
| L3 | M | **Texte arabe : mots collés** (72 caractères, 0 espace) ; poppler, MuPDF et pdf.js séparent les mots. La recherche d'un mot isolé échoue, la copie donne un bloc | corpus `polices/ArabicCIDTrueType.pdf` ; `[L19-ArabicCIDTrueType.pdf] espaces => 0` ; `with_word_spaces` (`text.rs:174`) ne crée pas d'espace, il n'élargit que ceux qui existent | dans `reading_order::layout_chars`, insérer un espace quand l'écart entre deux caractères d'une même ligne dépasse une fraction de la taille de police (ce que PDFium fait pour l'écriture de gauche à droite seulement) |
| L4 | F | `/ToUnicode` ignoré par PDFium sur une police Type1 **non incorporée** : « ABCUXABpq » au lieu de « óº€íºXABéê » | corpus `polices/nonembedded_type1_tounicode.pdf` | limite PDFium ; documenter ; un repli lopdf sur `/ToUnicode` (même mécanisme que L2) le couvrirait |
| L5 | F | Espacement de mots (`Tw`) d'une police Type3 non appliqué sur la première ligne : « abbaabba » au lieu de « ab ba abba » | corpus `polices/Type3WordSpacing.pdf` ; pdf.js et poppler séparent | limite PDFium ; documenter |
| L6 | I | Glyphes CID sans `/ToUnicode` : texte faux (déjà C7 du Lot 13) ; les trois moteurs donnent aussi du texte faux, différent | `polices/arial_unicode_ab_cidfont.pdf` | limite assumée, inchangée |
| L7 | I | Les valeurs de champs de formulaire et le contenu des `FreeText` ne font pas partie du texte de page (comme pdf.js ; poppler et MuPDF les incluent) : une recherche ne trouve pas une valeur saisie | `formulaires/annotation-text-widget.pdf`, `annotations/annotation-freetext.pdf`, `navigation/evaljs.pdf` | choix de conception à garder ; mentionner dans l'aide de la recherche |
| L8 | I | JPX avec canal alpha dans les données (`/SMaskInData`) rendu opaque par PDFium | `images/jpx_smaskindata.pdf` : 8 à 32 % de pixels différents des trois moteurs | limite PDFium amont |
| L9 | I | **Lecture plus tolérante que les autres** : un objet absent de la table xref mais présent dans le fichier est retrouvé par PDFium (signets « ressuscités », 0 → 2) et **matérialisé** à l'enregistrement ; qpdf, MuPDF et pdf.js le traitent comme `null` | corpus `fichier/append-page-content.pdf` : `/Outlines 95 0 R`, objet 95 hors xref | documenter ; un lecteur strict et nous ne verrons pas le même document |
| L10 | I | Fichier R6 à filtres `Identity` et xref abîmée : PDFium l'ouvre sans mot de passe (poppler aussi), qpdf, MuPDF et pdf.js exigent un mot de passe ; la sortie reproduit l'entrée | corpus `navigation/encrypted-attachment.pdf` | documenter |
| L11 | F | Annotations tierces : `Polygon` et `FileAttachment` **non rendues** (0 pixel), `FileAttachment` absente de `attachments()` ; `FreeText` tiers lue comme `Text` **éditable** : une modification la supprime et la recrée en `/Stamp` (perte du type d'origine et de `/DA`) | `[L15]` ; `annotations/mod.rs:848-879` | lister les `FileAttachment` dans les pièces jointes ; `editable: false` pour une annotation dont l'apparence n'est pas la nôtre ; rendu de `Polygon` : limite PDFium |
| L12 | F | Titre lu seulement dans `/Info` : un document avec XMP `dc:title` sans `/Info` affiche un titre vide (Acrobat et PDF 2.0 privilégient le XMP) | `[L17] XMP seul => None` | repli sur `dc:title` du XMP dans `properties()` |
| L13 | I | `/UserUnit` ignoré pour la taille de page (comme PDFium ; Acrobat et pdf.js l'appliquent à l'affichage) ; vertical CJK : ordre différent de poppler mais cohérent ; chaîne UTF-8 avec BOM (PDF 2.0) dans `/Title` : lue « Café » par nos deux décodeurs | `[L12]`, corpus `polices/vertical.pdf`, `[E20e]` | afficher la taille physique dans Propriétés quand `/UserUnit` ≠ 1 |

Écarts du corpus imputables aux moteurs de référence, **sans défaut de notre côté** : poppler
rend les images bilevel (CCITT, JBIG2, scans) sans lissage à 72 dpi (13 % de pixels) ; pdf.js sous
Node ne décode pas JPX (100 %) ni le CJK non incorporé ; MuPDF ne lit pas `append-xref-loop.pdf`
ni le texte des Type3 (il rend « □△ ») ; poppler perd des caractères CJK non incorporés.

## 4. Écriture : constats

| # | G | Constat | Preuve | Proposition |
|---|---|---|---|---|
| E1 | É | **Fuite sous le repli du nettoyage** : si `cleanup::before_save` ne peut pas relire la sortie de PDFium, les octets de `FPDF_SaveAsCopy` sont écrits tels quels, **sans élagage** : une annotation supprimée reste en objet orphelin avec son `/Contents` en clair (et le `/Alt` de son élément de structure). Reproductible avec un `/Encrypt` à filtres `Identity` ; se produirait pour toute sortie que lopdf ne relit pas. L'utilisateur croit la note supprimée | `[E2] fuites => ["litteral"]`, objets `4 0` et `8 0` ; `cleanup.rs:28-34` ; témoin sans `/Encrypt` : aucune fuite | deux étages : (1) `cleanup::can_clean(bytes, password)` (= `LopdfSession::load` réussit) appelé en tête de `delete_annotation`, `delete_pages`, `delete_object`, `update_annotation`, `set_annotation_rect` : refus clair, comme le caviardage et `check_regeneration_safe` ; (2) session « passe-plat » dans `crypt.rs` quand `/StmF` = `/StrF` = `Identity` (rien à déchiffrer, `/Encrypt` remis tel quel avant `prune`), ce qui lève aussi le refus L16 |
| E2 | É | **En-tête `%PDF-2.0` réécrit `%PDF-1.0`** par PDFium ; lopdf le recopie. Conséquence mesurée : **toute édition d'un PDF/A-4 ou d'un PDF/UA-2 le rend non conforme** (veraPDF 6.1.2-t1 « header shall be %PDF-2.n ») ; un lecteur strict peut refuser des structures 1.5+ dans un fichier « 1.0 » | `[E1a-2.0] en-tête => %PDF-1.0` ; corpus `pdfa/pdfa-4-*` (6 fichiers) et `pdfua/pdfua-2-*` (3) : PASS avant, FAIL après rotation, métadonnées, optimisation, filigrane | dans `before_save`, reposer la version d'en-tête d'origine (lue dans `file_bytes`) : `doc.version = …` avant `save_to` ; une ligne |
| E3 | É | **`merge`, `extract_pages`, `split` vident le catalogue** : `/AcroForm` (les widgets sont copiés sans formulaire : champs morts), `/Outlines`, `/Names` (destinations, JavaScript, **pièces jointes**), `/Dests`, `/PageLabels`, `/Lang`, `/OpenAction`, `/ViewerPreferences`, `/PageMode`, `/Metadata` XMP, `/OutputIntents`, `/OCProperties` (calques : un calque masqué devient visible), `/StructTreeRoot` ; `BDC`/`/OC` et `/StructParents` restent en références mortes. **Pire : le `/Parent` d'un widget copié pointe sur un objet quelconque du nouveau document** (ici un flux de contenu), parce que PDFium ne renumérote pas les clés `/Parent`, `/Prev`, `/First` (`cpdf_pageorganizer.cpp`) : un formulaire parent/enfant (iText, LibreOffice) fusionné est corrompu, `form_fields()` ne voit plus rien | `[E1f-merge]`, `[E1g-*]` : 17 structures perdues ; pdf.js : `isAcroForm false` avec un widget `Tx` ; `[E20d] Parent -> <</Filter /FlateDecode …>>stream`, `form_fields => []` ; `pages.rs:128-223` (document PDFium neuf) | après la copie PDFium, passe lopdf qui reporte depuis la source : pièces jointes, `/Lang`, `/ViewerPreferences`, `/PageMode`, `/Metadata`, `/OutputIntents`, `/MarkInfo` (clone) ; `/AcroForm` avec `/Fields` filtré aux champs dont un widget a survécu et **`/Parent` réassocié** ; signets filtrés et renumérotés, destinations nommées remappées, `/PageLabels` recalculés ; `/OCProperties` reconstruit à partir des OCG cités ; `/StructParents` retirés des pages si l'arbre n'est pas reporté. Chaque étape est une fonction pure sur `lopdf::Document`, testable sur `rich_catalog_pdf` |
| E4 | M | **`move_pages` remplace la page par une copie** : tout ce qui visait l'ancien objet (signets, destinations nommées, liens d'autres pages) tombe à `None` ; sur la page copiée, les liens à destination **explicite** (`/Dest [page …]`, `/A /GoTo /D [page …]`) perdent leur cible (PDFium retire une clé dont la référence vise une page) ; le widget copié n'est plus dans `/AcroForm /Fields` (qpdf : « widget annotation is not reachable from /AcroForm »). Les `/Annots`, le balisage et `/StructParents` sont bien copiés (remesuré : E20a, contredit la première mesure E1d). `delete_pages` laisse les signets vers la page supprimée en l'état (cible `None`) mais **n'écrit pas de page fantôme** (E20c : 2 pages dans l'arbre, 2 dictionnaires de page) | `[E1d] signets => None` ; `[E20a] liens p3 (ex-p1) => 2 sur 3, qpdf warnings 1` ; `pages.rs:104-122` ; ADR-0007 ne documente que les signets | faire le déplacement **par lopdf** : retirer la référence de `/Kids`, l'insérer à la cible, mettre `/Count` et `/Parent` à jour, recopier les attributs hérités (§7.7.3.4) ; la page reste le même objet et rien n'est à remapper ; puis `reload_from_bytes`. Pour `delete_pages`, neutraliser dans `before_save` les destinations dont la page n'est plus dans l'arbre |
| E5 | F | `regenerate_content` conserve les marques (`BDC`/`EMC`, `/MCID`, `/OC`) des objets qui survivent (remesuré : E20b, contredit la première mesure E1e où l'objet supprimé était le seul de la page) ; seule la marque de l'objet supprimé disparaît, et l'arbre de structure cite alors un MCID absent ; **important** pour un document déclaré PDF/UA | `[E20b] MCID 0 / MCID 1 => false / true` ; `content/mod.rs:296-375` | passe `before_save` sur les documents à `/StructTreeRoot` : retirer de `/K` les MCID absents du contenu de la page (même logique que `scrub_links` pour `/OBJR`) |
| E6 | M | **Édition d'un PDF/A : le filigrane (et donc les numéros de page et l'OCR, même flux) casse toutes les conformités** : opacité `/CA` `/ca` ≠ 1 (interdite en PDF/A-1), DeviceRGB sans `OutputIntent` ni `DefaultRGB` (2b, 2u), transparence sans `/Group` de page, `/CIDSet` absent du sous-ensemble DejaVu (1b) ; et **non balisé en `/Artifact`** (PDF/UA-1 7.1-t3). La modification des métadonnées casse PDF/A-1b quand le XMP n'a pas de `dc:title` à synchroniser (6.7.3-t2) ; tout `/ModDate` ajouté casse PDF/A-4 (6.1.3-t4 : `/Info` interdit) | veraPDF sur les 16 `pdfa` et 7 `pdfua` du corpus : rotation et optimisation conservent 1b, 2b, 2u, 3b, UA-1 ; filigrane : 11 FAIL sur 16 ; métadonnées : 4 FAIL (1b) ; PDF/A-4 et UA-2 : tout FAIL (voir E2) ; clauses dans `target/audit/verapdf-clauses.txt` | envelopper filigrane, numéros et OCR dans `/Artifact BMC … EMC` ; quand le document déclare PDF/A : pas d'opacité en A-1, `/CIDSet` dans le sous-ensemble, couleur en `/DeviceGray` ou via l'OutputIntent, `/Group` de page ; écrire `dc:title` dans le XMP même absent ; ne pas créer `/Info` sur un PDF/A-4 (ou seulement `/ModDate`) ; ADR-0018 promettait un avertissement, l'avertissement existe, mais on peut faire mieux sans effort disproportionné |
| E7 | F | `/NeedAppearances true` posé à chaque `SetFieldValue`, même absent en entrée, et laissé dans le fichier (Acrobat propose alors d'enregistrer à l'ouverture ; interdit en PDF/A) ; PDFium a pourtant déjà régénéré les `/AP` | `[E3]` : 4 variantes, toujours `true` ; `/AP` présent dans les 4 ; `forms/write.rs:104-132` | retirer `/NeedAppearances` dans `before_save` quand chaque widget modifié a un `/AP` |
| E8 | F | AES-256 R6 posé sur un fichier `%PDF-1.4` sans relever `/Version` ni poser `/Extensions` (R6 est PDF 2.0 ; R5/R6 étaient l'extension Adobe niveau 3/8 en 1.7) ; qpdf, MuPDF et pdf.js ouvrent ; Acrobat : attendu OK (il lit `/R`), non vérifié | `[E7]` ; ADR-0011 §9 l'assume | dans `protect`, en-tête relevé à 1.7 s'il est inférieur et `/Extensions << /ADBE << /BaseVersion /1.7 /ExtensionLevel 8 >> >>` dans le catalogue (Adobe Supplement ; 2.0 une fois E2 corrigé) |
| E9 | F | `insert_image` **réencode un JPEG en Flate** (15 Ko → 85 Ko) et ajoute un `/SMask` opaque de 52 octets même sans transparence ; le PNG est correct (SMask réel) | `[E8]` ; `content/image.rs:22-24` | passer les octets JPEG tels quels (`/DCTDecode`) par une passe lopdf après `create_image_object`, ou refuser le SMask quand l'image est opaque |
| E10 | F | Police Unicode **entière** incorporée par le chemin PDFium (`insert_text`, texte libre, tampon hors WinAnsi) : `/FontFile2` de 386 Ko (DejaVu 757 Ko), fichier de 2 Ko → 420 Ko ; le chemin lopdf (`Type0Writer`, filigrane, OCR) écrit un sous-ensemble de 7 Ko (mesuré sur le corpus : +7,4 Ko par filigrane « Ω ») | `[E6]` ; ADR-0014 §4 l'assume | sous-ensembler dans `before_save` : retirer les glyphes non référencés par les `/W`/CID utilisés (la bibliothèque `subsetter` est déjà là) ; ou router ces textes par `Type0Writer` |
| E11 | F | Annotations créées sans `/AP` : `/Text` (retiré volontairement) et **`/Highlight`** (jamais généré quand on ajoute puis enregistre sans relecture). pdf.js et MuPDF les dessinent eux-mêmes ; Acrobat aussi ; **PDF/A exige un `/AP`** (6.3.3) | `[E10]` : `Text hasAppearance false`, `Highlight hasAppearance false` (pdf.js) ; `annotations/mod.rs:394` (rendu minuscule seulement à la lecture) | dans `rewritten_bytes` (`edit.rs:823`), avant `save_to_bytes`, faire sur chaque page qui a un marquage sans apparence le rendu 8 px déjà utilisé par `annotations()` : PDFium écrit alors un `/AP` portable (`/BM /Multiply`), que `cleaned()` conserve |
| E12 | I | `/ID` : fichier existant → premier identifiant conservé, second renouvelé (conforme) ; document neuf (`merge`, `extract`, `split`) → les deux égaux (conforme à la création) | `[E4]`, `[E1a]` | aucune |
| E13 | I | Flux lopdf (filigrane, OCR) écrits en clair puis **recomprimés par PDFium** au rechargement : 0 flux sans filtre dans la sortie finale | `[E5]` | aucune |
| E14 | I | Signatures : `adbe.pkcs7.sha1`, `adbe.x509.rsa_sha1`, `ETSI.RFC3161` → `Unsupported` sans erreur ; trois signatures successives → toutes `Intact`, seule la dernière `covers_whole_document` ; aucune chaîne de confiance, révocation ni horodatage (ADR-0013) | `[L18]` | limite assumée ; deux retouches simples : verdict « certification invalidée » quand `/DocMDP /P 1` et qu'une révision suit (`!covers_whole_document`), et `ETSI.RFC3161` présenté comme horodatage de document (ISO 32000-2 §12.8.5), non comme signature |
| E15 | I | Un objet retrouvé par tolérance (L9) est écrit dans la sortie : le fichier enregistré « contient plus » que ce qu'un lecteur strict voyait | corpus `append-page-content.pdf` : signets 0 → 2 | documenter avec L9 |

## 4 bis. Lecture normative (agent `pdf-expert`)

Les 17 constats synthétiques ont été relus contre ISO 32000-1:2008, ISO 32000-2:2020, PDF/A et
PDF/UA, et contre la source de PDFium, pdf.js, lopdf et pdfium-render. Deux mesures ont été
contestées et **remesurées** (E20a, E20b : §4, E4 et E5 corrigés) ; trois points non mesurés ont été
ajoutés (E20c : pas de page fantôme après `delete_pages` ; E20d : `/Parent` des widgets corrompu
par `merge` ; E20e : UTF-8 avec BOM lu correctement). Ce qui change la lecture des constats :

| Constat | Clause | Ce que font les autres lecteurs | Gravité retenue |
|---|---|---|---|
| E1 fuite sous repli | §7.3.10 tolère un objet orphelin ; seul le caviardage (32000-2 §12.5.6.23) exige la disparition ; c'est une promesse du projet (Lot 9, B20) | aucun lecteur n'affiche l'orphelin ; `qpdf --show-object` et tout éditeur le lisent | **É** confirmée (toute suppression, dès que lopdf ne relit pas : pas seulement `Identity`) |
| E2 en-tête 1.0 | §7.5.2, Tableau 28 ; PDF/A-4 et UA-2 exigent 2.0 ; PDF/A-2/3 exigent 1.n | tous ouvrent ; Acrobat affiche « version 1.0 » ; veraPDF échoue | **É** confirmée ; `cpdf_creator.cpp` ne sait écrire que 1.0 à 1.7 : la version se repose après coup (même longueur, offsets intacts) |
| E3 catalogue vidé | Tableau 28 ; §7.11.4 ; §8.11.4.1 (`/OCProperties` requis si contenu optionnel) ; §12.7.2 ; §12.7.3.1 (`/Parent`) | Acrobat : formulaire « sans champs », calques tous visibles, pièces jointes absentes ; pdf.js idem (mesuré) | **É** confirmée ; `/Parent` dangling ajouté (E20d) |
| E4 `move_pages` | §7.7.3.2, §7.7.3.4, §12.3.2.2 | signet vers une page remplacée : Acrobat ne fait rien au clic, pdf.js et MuPDF ignorent | **M** ; correction par réordonnancement `/Kids` lopdf (la page reste le même objet) |
| E5 balisage | §14.6, §14.7.4.2, §14.7.4.4 | Acrobat tolère un MCID manquant (élément vide) | **F**, important si PDF/UA déclaré |
| E6 PDF/A | PDF/A-1 6.4 (opacité), 6.3.5 (`/CIDSet`) ; PDF/A-2 6.2.4.3, 6.2.10, 6.4.1-3 (`/NeedAppearances`), 6.3.3-1 (`/AP`) ; PDF/A-4 6.1.2, 6.1.3 ; PDF/UA-1 7.1 (artefacts) | veraPDF (mesuré) | **M** confirmée |
| E7 `/NeedAppearances` | Tableau 218 ; PDF/A-2 6.4.1-3 | Acrobat régénère et propose d'enregistrer ; pdf.js et MuPDF régénèrent | **F** |
| E8 AES-256 en 1.4 | 32000-2 §7.6.4 ; §7.12 `/Extensions` (ExtensionLevel 8 = R6) | Acrobat X+, pdf.js, MuPDF, qpdf lisent `/R` sans regarder l'en-tête ; Aperçu avant 10.13 incertain | **F** |
| E9 JPEG | §7.4.8, §11.6.5.3 ; cause : pdfium-render crée toujours un bitmap BGRA, `CPDF_Image::SetImage` pose un `/SMask` dès qu'il y a un canal alpha | tous affichent ; un `/SMask` opaque force un aplatissement à l'impression | **F** |
| E10 police entière | §9.9 ; §9.6.4 n'exige l'étiquette que pour un sous-ensemble : conforme | aucun problème, fichier lourd | **F** |
| E11 `/AP` absent | 32000-1 : facultatif ; 32000-2 Tableau 166 et PDF/A-2 6.3.3-1 : requis sauf `Popup` et `Link` | Acrobat, pdf.js (`blendMode Multiply`), MuPDF, PDFium synthétisent le surlignage ; tous dessinent l'icône d'une note | **F**, important en PDF/A ou PDF 2.0 |
| L11 annotations tierces | §12.5.6.6 (`FreeText` : `/DA` requis, texte éditable sémantiquement) ; §12.5.6.15 + §7.11.3 (`FileAttachment` = pièce jointe) ; PDFium ne génère d'apparence que pour Circle, FreeText, Highlight, Ink, Popup, Square, Squiggly, StrikeOut, Text, Underline | `Polygon` sans `/AP` : Acrobat, pdf.js, MuPDF dessinent, nous non ; `FileAttachment` : Acrobat et pdf.js la listent | **M** pour `FreeText` (transformation destructive en `/Stamp`) et `FileAttachment` ; **F** pour `Polygon` et `Popup` |
| L4, L6, L13 extraction | §9.10.2 : code → nom (`/Differences`) → AGL, la réponse normative est « B » ; CID sans `/ToUnicode` : aucune correspondance définie | Acrobat, pdf.js, MuPDF extraient « B » | **F**, limites PDFium |
| L12 titre XMP | 32000-2 §14.3.3 déprécie `/Info` sauf les dates : un producteur 2.0 peut n'écrire que le XMP | Acrobat et pdf.js préfèrent `dc:title` ; MuPDF ne lit que `/Info` | **F** |
| E12 `/ID` | §14.4 : « à la première écriture, les deux identifiants shall être identiques » | — | **I**, conforme |
| E14 signatures | §12.8.1 (seule la dernière signature couvre tout : normal) ; §12.8.2.2 (`/DocMDP`) ; 32000-2 §12.8.5 (`ETSI.RFC3161` = horodatage de document) | Acrobat vérifie chaîne, révocation, DocMDP ; pdf.js et Aperçu ne vérifient rien | **I** ; **F** pour DocMDP `P 1` |
| L16 `/StmF /Identity` | §7.6.5 Tableau 25 (`Identity` = passe-plat), Tableau 20 (`/EFF`) | tous ouvrent | **I** ; se corrige avec E1 (2) |

## 5. Couverture de `tests/conformance.rs`

Les commandes suivantes n'y passent pas par `qpdf --check` ; le banc les a toutes vérifiées
(E9, 0 alerte) et elles doivent y entrer au Lot 14 pour rester vraies : `unprotect` ; `set_field_value`
sur case, radio, combo ; `add_annotation` ellipse, flèche, souligné, barré, ondulé, tampon (ASCII et
Unicode) ; `watermark`, `page_numbers`, OCR hors WinAnsi (`Type0Writer`) ; `insert_text` Standard 14 ;
`set_object_text` avec remplacement DejaVu ; `insert_image` JPEG et PNG alpha ; `insert_signature`
image ; `optimize` avec recompression effective ; toute édition sur `encrypted.pdf` et `owner-only.pdf`
(qpdf avec mot de passe) ; signature visible ; `merge`, `split`, `extract_pages`.

À ajouter aussi : un test de **conservation du catalogue** (générateur `rich_catalog_pdf` du banc,
17 structures) après chaque variante d'`EditCommand`, qui aurait révélé E2 à E5 ; et une relecture
`mutool`/`pdfjs` optionnelle (variables `MEGA_MUTOOL`, `MEGA_PDFJS_CHECK`, sautée si absentes).

Sur ce poste, qpdf était installé **hors du PATH** et `MEGA_QPDF` non défini : `conformance.rs`
se sautait en silence depuis le début (`cargo test` vert sans aucun contrôle). Avec `MEGA_QPDF`
posé, les 7 tests passent et appellent bien qpdf. Proposition : afficher un avertissement visible
(et non un simple `println!`) quand le test se saute, et documenter `MEGA_QPDF` dans `CONTRIBUTING.md`.

## 6. Documentation périmée ou à compléter

- ADR-0008 : les paragraphes « pas d'`/InkList` » et « `prune_objects` au Lot 5 » sont périmés
  (réglés par `cleanup`) ; ajouter que `/Highlight` n'a pas d'`/AP` (E11).
- ADR-0009 : « documents chiffrés non modifiables » périmé (ADR-0014 §7).
- ADR-0010 : « toute passe lopdf s'abstient sur un fichier chiffré » périmé ; ajouter E5.
- ADR-0011 : « mots de passe Latin-1 » périmé (ADR-0018) ; ajouter E8.
- ADR-0013 : « OCR limité à WinAnsi » périmé (Type0 depuis ADR-0014).
- ADR-0007 : `move_pages` perd aussi les **liens** (E4), pas seulement les signets.
- ADR-0018 §7 : compléter par les mesures veraPDF (E6) : ce qui conserve PDF/A (rotation,
  suppression, optimisation, métadonnées sur 2b+) et ce qui le casse.
- `docs/FONCTIONNALITES.md` : « à confirmer manuellement dans Acrobat / PDF.js » → pdf.js et MuPDF
  sont maintenant vérifiés par le banc.

## 7. Proposition de Lot 14 « Compatibilité mesurée »

Par ordre de gravité, groupé en PR indépendantes. Les corrections de gravité **F** restent
optionnelles ; les **I** ne demandent que de la documentation.

| PR | Contenu | Constats | Effort |
|---|---|---|---|
| **A. Enregistrement sûr et fidèle** | `can_clean` avant toute suppression et session passe-plat `Identity` ; version d'en-tête d'origine reposée ; `/Info` non créé sur PDF/A-4 ; `merge`/`extract`/`split` reportent pièces jointes, `/Lang`, préférences, XMP, OutputIntents, AcroForm (avec `/Parent` réassocié), signets, destinations nommées, `/PageLabels`, `/OCProperties` ; `move_pages` par réordonnancement `/Kids` lopdf ; `delete_pages` neutralise les destinations orphelines ; MCID orphelins élagués ; test de conservation du catalogue dans `conformance.rs` | E1, E2, E3, E4, E5, L16 | 4 à 5 jours |
| **B. Texte extrait et annotations tierces** | paires de substitution ; espaces de mots en écriture de droite à gauche ; police CFF CID OpenType (fixture minimale, repli `/ToUnicode` lopdf si PDFium reste muet) ; titre depuis le XMP ; `FileAttachment` dans les pièces jointes ; `FreeText` tiers non éditable ; `Popup` masquée de la liste ; `/UserUnit` dans Propriétés | L1, L2, L3, L4, L11, L12, L13 | 2 à 3 jours |
| **C. Ajouts respectueux de PDF/A et PDF/UA** | `/Artifact` autour des filigranes, numéros et OCR ; `/CIDSet` ; couleur et opacité adaptées quand le document déclare PDF/A ; `/Group` de page ; `dc:title` écrit même absent ; `/NeedAppearances` retiré après régénération ; `/AP` du surlignage ; `/Extensions` et en-tête 1.7 au chiffrement AES-256 ; verdict DocMDP ; tests veraPDF dans `conformance.rs` (sautés sans `MEGA_VERAPDF`) | E6, E7, E8, E11, E14 | 2 jours |
| **D. Taille des fichiers** | image écrite par lopdf (`/DCTDecode` tel quel, `/SMask` seulement si alpha réel, recette de `raster.rs`) ; sous-ensemble des `/FontFile2` DejaVu de PDFium dans `before_save` (GID conservés, étiquette `XXXXXX+`) | E9, E10 | 1 à 2 jours |
| **E. Conformance et documentation** | les 20 commandes manquantes dans `conformance.rs` ; relecture `mutool`/pdf.js optionnelle ; avertissement visible quand qpdf manque ; ADR mis à jour (§6) ; `docs/FONCTIONNALITES.md` et aide : L7, L9, L10, L13, E12 à E15 | §5, §6 | 1 jour |

Resteront **limites assumées** : glyphes CID sans `/ToUnicode` (L6), rendu de `Polygon` et du JPX
à alpha intégré (PDFium amont), `Tw` des Type3 et `/ToUnicode` des Type1 non incorporées (PDFium),
signatures sans chaîne de confiance ni horodatage, tolérance de lecture de PDFium (L9, L10).

Fixtures à prévoir au Lot 14 (générateurs déjà dans le banc) : `rich_catalog_pdf("2.0")`,
`two_marked_objects_pdf`, `identity_encrypted_note_pdf`, `third_party_annots_pdf`, `sig_subfilter_pdf`,
plus une page avec `/FileAttachment /FS`, un `FreeText` tiers, et `form_parent_kid_pdf` fusionné.

## 8. Annexe : rejouer les mesures

```
# outils (voir §1) ; dans Git Bash, depuis la racine du dépôt
export MEGA_QPDF='C:\Program Files\qpdf 12.4.2\bin\qpdf.exe'
export MEGA_MUTOOL="$PWD/tools/dl/mupdf/mutool.exe"
export MEGA_PDFJS_CHECK="$PWD/scripts/audit/pdfjs-check.mjs"      # pdfjs-dist dans tools/pdfjs
export MEGA_VERAPDF="$PWD/tools/verapdf/verapdf.bat"
bash scripts/audit/fetch-corpus.sh                                 # fixtures/corpus/ (137 fichiers)
cargo build -p mega-cli
bash scripts/audit/read-bench.sh                                   # target/audit/lecture.csv
bash scripts/audit/write-bench.sh                                  # target/audit/ecriture.csv
# banc synthétique retiré au Lot 14 : le relancer depuis un arbre de travail à sa révision
git worktree add ../banc-audit e2d8338 && cd ../banc-audit
cargo test -p mega-core --test audit_compat -- --ignored --nocapture --test-threads=1 \
  > target/audit/audit_compat.txt                                  # verdicts [E…] et [L…]
tools/verapdf/verapdf.bat --flavour 0 --format text fixtures/corpus/pdfa/*.pdf      # avant
tools/verapdf/verapdf.bat --flavour 0 --format text target/audit/ecriture/pdfa-*/filigrane.pdf
tools/verapdf/verapdf.bat --flavour ua1 --format text target/audit/ecriture/pdfua-pdfua-1-*/filigrane.pdf
MEGA_REQUIRE_QPDF=1 cargo test -p mega-core --test conformance    # vérifie que qpdf est bien appelé
```

Sorties intermédiaires : `target/audit/lecture/<famille>-<fichier>/{mega,poppler,mupdf,pdfjs}.{txt,png}`,
`target/audit/ecriture/<famille>-<fichier>/{rotation,filigrane,metadonnees,optimisation}.pdf`,
`target/audit-compat/*.pdf` (sorties du banc synthétique, à inspecter avec `qpdf --json` ou `mutool show`).

## 9. Ce que sont devenues les expériences du banc (Lot 14, tâche 23)

Le banc `audit_compat.rs` imprimait des verdicts sans jamais échouer ; il est retiré. Chaque
expérience est reprise par des tests qui **affirment** (fichiers sous `crates/mega-core/tests/`,
« unitaire » = module de `src/`).

| Expérience | Test(s) |
|---|---|
| E1 catalogue riche | `edit.rs::untouched_save_writes_the_loaded_bytes` ; `cleanup.rs::before_save_keeps_a_2_0_header` ; `pages.rs::rotate_merge_extract_and_protect_keep_the_2_0_header`, `catalog_version_entry_survives_a_rewrite` ; `conformance/catalogue.rs::every_edit_command_keeps_the_rich_catalog`, `merge_extract_and_split_carry_the_promised_catalog` |
| E2 repli du nettoyage | `cleanup.rs::deleted_note_does_not_survive_in_an_identity_encrypted_file`, `can_clean_accepts_identity_pass_through_and_plain_files_but_not_unknown_filters`, `every_removing_operation_is_refused_on_an_uncleanable_file` ; témoin : `redact.rs::deleting_a_note_also_drops_it_from_the_saved_file` |
| E3 `/NeedAppearances` | `forms.rs::need_appearances_is_dropped_when_every_widget_has_an_appearance` (les quatre variantes) |
| E4 `/ID` | `saving.rs::an_edited_file_keeps_its_first_id_and_renews_the_second` ; `encrypted.rs::identity_filtered_file_keeps_its_encrypt_dictionary_and_attachment_bytes_after_watermark` ; `digital_signature.rs::signed_revision_renews_the_second_id` |
| E5 recompression des flux `lopdf` | `saving.rs::streams_written_by_edits_are_compressed_in_the_saved_file` |
| E6 police DejaVu entière | `content.rs::inserted_unicode_text_embeds_a_subset_font` ; `annotations.rs::two_unicode_annotations_share_one_subset` |
| E7 AES-256 sur `%PDF-1.4` | `security.rs::protect_declares_the_adobe_extension_level_and_a_1_7_header` |
| E8 `insert_image` | `content.rs::inserted_jpeg_keeps_dct_encoding_and_has_no_smask`, `inserted_png_with_alpha_gets_a_real_smask`, `inserted_opaque_png_has_no_smask`, `inserted_image_on_a_rotated_page_lands_at_the_displayed_rectangle` |
| E9 qpdf hors `conformance` | `conformance/edits.rs` (formulaires, genres d'annotation, texte Unicode et images, fichiers chiffrés y compris caviardage, optimisation, `every_edit_command_variant_is_checked_by_qpdf`), `conformance/signature.rs::a_visible_signature_produces_a_conforming_file`, `conformance/pages.rs::merge_split_and_extract_produce_conforming_files` |
| E10 annotations créées | `annotations.rs::highlight_added_then_saved_has_an_appearance_stream` ; `conformance/edits.rs::every_annotation_kind_produces_a_conforming_file` |
| L11 arbre profond, héritage | `conformance/pages.rs::rotating_a_page_with_an_inherited_rotation_adds_to_it` ; `overlay.rs::inherited_media_box_is_read` ; `conformance/pages.rs::moving_pages_across_page_tree_nodes_produces_conforming_files` ; unitaire `pages/reorder.rs::reorder_kids_copies_inherited_attributes_before_reparenting` |
| L12 `/UserUnit` | `metadata.rs::user_unit_is_reported`, `user_unit_accepts_reals_and_is_bounded` |
| L13 destinations nommées | `pages.rs::merging_two_rich_catalogs_keeps_bookmarks_attachments_fields_and_links`, `moving_a_page_keeps_its_links_tagging_and_the_bookmarks_to_it` |
| L14 encodages de police | `document.rs::a_differences_encoding_is_followed_for_a_standard_font`, `a_symbolic_truetype_font_is_read_by_its_code` (limite L4), `glyphs_without_unicode_keep_their_place`, `astral_characters_are_extracted_searchable_and_copied`, `opentype_cff_cid_text_is_extracted` |
| L15 annotations tierces | `annotations.rs::reads_third_party_annotations`, `third_party_free_text_is_shown_but_not_editable`, `popups_are_not_listed` ; `attachments.rs::file_attachment_annotations_are_listed_with_their_page_and_extracted` |
| L16 `/StmF /Identity` | `encrypted.rs::identity_filtered_file_keeps_its_encrypt_dictionary_and_attachment_bytes_after_watermark`, `identity_pass_keeps_the_encrypt_dictionary_unchanged` |
| L17 métadonnées | `metadata.rs::title_falls_back_to_xmp_dc_title_when_info_is_absent` ; unitaire `properties::pdfdoc_encoding_bullet_euro_and_ligatures` |
| L18 signatures | `digital_signature.rs::rfc3161_subfilter_is_reported_as_a_document_timestamp`, `second_signature_keeps_the_first_valid`, `third_party_pades_signature_verifies` |
| L19 corpus réel | sous `MEGA_CORPUS` : `conformance/corpus.rs::corpus_text_samples_are_extracted` ; `document.rs::astral_characters_in_the_corpus`, `opentype_cff_cid_corpus_file_has_text`, `arabic_corpus_file_has_word_spaces` |
| E20 remesures | a : `pages.rs::moving_a_page_keeps_its_links_tagging_and_the_bookmarks_to_it` ; b : `cleanup.rs::deleting_one_of_two_marked_objects_prunes_its_mcid_from_the_structure_tree` ; c : `cleanup.rs::bookmarks_to_a_deleted_page_lose_their_destination_but_remain` ; d : `pages.rs::a_merged_parent_kid_form_keeps_its_field` ; e : unitaire `properties::utf8_bom_text_is_decoded` |
