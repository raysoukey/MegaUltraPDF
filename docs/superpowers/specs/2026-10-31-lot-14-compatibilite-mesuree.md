# Lot 14 — Compatibilité mesurée : corriger ce que l'audit a prouvé

## Contexte

L'audit `docs/audit/2026-10-31-compatibilite-pdf.md` (PR #24) a confronté la v0.13.0 à 137 PDF
réels et à quatre moteurs (poppler, MuPDF, pdf.js, veraPDF) et a mesuré 13 constats en lecture et
15 en écriture, relus contre ISO 32000 par l'agent `pdf-expert`. Le propriétaire a retenu la
**proposition complète** du §7 : cinq PR, par gravité décroissante.

Intention : qu'un fichier enregistré par MegaUltraPDF soit **ce que l'utilisateur croit avoir
enregistré** (rien de supprimé qui survive, rien de perdu qu'il n'ait pas demandé, version et
déclarations de conformité respectées), et que le texte lu soit celui que les autres lecteurs
lisent. Release **v0.14.0** à la fin.

Le banc de mesure `crates/mega-core/tests/audit_compat.rs` (21 expériences `#[ignore]`) reste le
juge : chaque correction convertit l'expérience correspondante en **vrai test** (assertion), dans le
fichier de test du module concerné ; le banc est supprimé à la fin du lot, une fois tous ses
points couverts.

## Périmètre

**Entre dans le lot** (numéros du rapport) :

- PR A, enregistrement sûr et fidèle : E1, E2, E3, E4, E5, L16.
- PR B, texte extrait et annotations tierces : L1, L2, L3, L11, L12, L13 (`/UserUnit`).
- PR C, ajouts respectueux de PDF/A et PDF/UA : E6 (partie réalisable), E7, E8, E11, E14.
- PR D, taille des fichiers : E9, E10.
- PR E, conformance et documentation : §5 et §6 du rapport, L4 à L10, E12, E13, E15.

**N'entre pas** (limites assumées, documentées en PR E) : glyphes CID sans `/ToUnicode` (L6),
rendu de `Polygon` et du JPX à alpha intégré, `Tw` des Type3 et `/ToUnicode` des Type1 non
incorporées (PDFium), chaîne de confiance et horodatage des signatures, tolérance de lecture de
PDFium (L9, L10), report de `/StructTreeRoot` à la fusion (hors de portée raisonnable : on retire
les `/StructParents` orphelins), couleur et opacité d'un filigrane sur un PDF/A (l'avertissement
existant à l'enregistrement reste la réponse), signature des paquets Windows.

## Processus (inchangé)

- Branche `lot-14-compatibilite` (créée depuis `audit-compatibilite`, qui porte le banc et les
  scripts ; la PR #24 est fusionnée avant celle-ci, ou celle-ci l'inclut).
- Une PR par lettre (A à E), chacune `/verif` 8/8, relue par `rust-reviewer` (et `ui-reviewer`
  pour les touches `app/src`), CI verte sur les trois OS avant fusion.
- TDD strict : chaque tâche part d'un test rouge écrit à partir de la mesure du banc.
- `MEGA_QPDF` posé dans la session et `MEGA_REQUIRE_QPDF=1` sur la CI (déjà le cas).
- `/note-apprentissage` puis `/verif`, PR finale de version, release v0.14.0 avec contrôle du
  paquet installé.

## A. Enregistrement sûr et fidèle

### A1. Plus de fuite sous le repli du nettoyage (E1, L16)

- `cleanup::can_clean(bytes: &[u8], password: Option<&str>) -> bool` (`pub(crate)`) : vrai si
  `LopdfSession::load` réussit. Appelé en tête de `Document::delete_annotation`,
  `delete_pages`, `delete_object`, `update_annotation`, `move_annotation`
  (toute opération qui supprime ou recrée) : sinon
  `CoreError::InvalidOperation("ce fichier ne peut pas être nettoyé après une suppression…")`,
  même famille de message que `check_regeneration_safe` (`content/mod.rs:185`). L'insertion
  reste possible.
- `crypt::LopdfSession` gagne un mode **passe-plat** : quand `/StmF` et `/StrF` valent
  `Identity` (chaînes et flux en clair, seules les pièces jointes chiffrées par `/EFF`), la
  session charge sans déchiffrer, et `finish` remet le dictionnaire `/Encrypt` et `/ID` tels
  quels **avant** `prune`, sans appeler `encrypt`. Filigrane, métadonnées, suppression deviennent
  possibles sur ces fichiers ; les flux de pièces jointes restent des octets opaques.
- Tests (depuis `[E2]` et `[L16]`) : `tests/cleanup.rs` : `identity_encrypted_note_pdf` →
  `delete_annotation` puis `save_as` → la chaîne secrète est **absente** des octets ; la session
  passe-plat conserve `/Encrypt`, `/ID` et le flux `/EF` octet pour octet ; un fichier que lopdf
  ne relit pas (fixture `robustesse/` abîmée après réécriture forcée, ou `/Encrypt` à filtre
  inconnu) → `delete_annotation` refusé avec `InvalidOperation`, `add_annotation` accepté.

### A2. Version d'en-tête d'origine (E2)

- `crypt::header_version(bytes: &[u8]) -> Option<(u8, u8)>` : fonction pure qui lit `%PDF-M.n`
  après un éventuel préfixe (déjà géré par `load_limited`). `Document` mémorise
  `file_version` à l'ouverture (`file_bytes`) et à chaque `save_as`.
- `cleanup::before_save(bytes, password, min_version: Option<(u8, u8)>)` : pose
  `doc.version` au maximum de la version lue dans `bytes` et de `min_version` avant `save_to`.
  Le repli brut (octets PDFium tels quels) recopie les octets 5-7 de l'en-tête d'origine quand ils
  sont supérieurs (même longueur, offsets intacts).
- `merge` prend le maximum des sources ; `extract_pages`, `split`, `protect`, `sign_to` passent la
  version du document.
- Tests (`[E1a-2.0]`, `[E1a-1.4-v1.7]`) : `tests/cleanup.rs` et `tests/pages.rs` : entrée
  `%PDF-2.0` → sortie `%PDF-2.0` après rotation, fusion, extraction, protection ; entrée 1.4 avec
  `/Version /1.7` → `/Version` conservé ; `conformance.rs` : veraPDF (si `MEGA_VERAPDF`) garde
  `PASS` sur `fixtures/corpus/pdfa/pdfa-4-*` après rotation (test sauté sans corpus).

### A3. Pas d'`/Info` créé sur un PDF/A-4 (E6, partie)

- `properties::stamp_mod_date` : si la déclaration PDF/A est de partie 4, ne pas créer `/Info` ;
  si `/Info` existe déjà avec seulement `/ModDate`, le mettre à jour ; `xmp:ModifyDate` reste la
  référence. Test : `pdfa` partie 4 sans `/Info` → sortie sans `/Info`.

### A4. Report du catalogue après fusion, extraction, scission (E3)

Nouveau module `crates/mega-core/src/pages/carry.rs` (fonctions pures sur `lopdf::Document`,
testées sans PDFium sur `testing::rich_catalog_pdf`) :

- `PageMap` : correspondance `ObjectId` source → `ObjectId` cible, construite par index
  (les pages copiées par PDFium sortent dans l'ordre demandé).
- `carry_simple(src, dst)` : clone récursif avec table de correspondance (`clone_object`) de
  `/Lang`, `/ViewerPreferences`, `/PageMode`, `/PageLayout`, `/Metadata`, `/OutputIntents`,
  `/MarkInfo`, `/Names /EmbeddedFiles` (et `/Names /JavaScript` ? **non** : jamais reporté,
  décision : on n'importe pas de script).
- `carry_acroform(src, dst, map)` : `/DA`, `/DR`, `/NeedAppearances` ; `/Fields` filtré aux
  champs dont au moins un widget a survécu ; **`/Parent` des widgets copiés réassocié** au champ
  cloné (table d'objets) ; au `merge`, noms qualifiés rendus uniques par suffixe ` (2)`.
- `carry_outlines(src, dst, map)` : signets dont la cible (explicite ou nommée) est copiée,
  renumérotés ; les autres retirés ; `/Count` recalculé.
- `carry_named_dests(src, dst, map)` : `/Names /Dests` et `/Dests` filtrés et remappés.
- `carry_page_labels(src, dst, pages)` : étiquettes recalculées pour les pages conservées.
- `carry_oc_properties(src, dst)` : OCG cités par les ressources copiées (`/Properties`, `/OC`
  des XObjects et annotations), `/OCGs` et `/D` (`/ON`, `/OFF`, `/Order`) reconstruits.
- `strip_struct_parents(dst)` : retire `/StructParents` des pages et `/StructParent` des
  annotations quand `/StructTreeRoot` n'est pas reporté.
- Branchement : `pages::merge`, `Document::extract_pages`, `pages::split` appliquent la passe
  après la copie PDFium puis `before_save`. Tests (`[E1f]`, `[E1g]`, `[E20d]`) : 17 structures
  présentes après `merge` de deux `rich_catalog_pdf` ; `form_fields` non vide et `/Parent` → champ
  après `merge(form_parent_kid_pdf, simple)` ; pièce jointe conservée après `extract_pages` ;
  `qpdf --check` 0 alerte.

### A5. Déplacement de pages sans copie (E4)

- `pages::reorder_kids(doc: &mut lopdf::Document, pages: &[u16], to: u16) -> lopdf::Result<()>`
  (pure, testée) : retire les références de `/Kids`, les insère à la cible, met `/Count` à jour
  jusqu'à la racine, pose `/Parent`, recopie sur la page les attributs hérités résolus
  (`/Resources`, `/MediaBox`, `/CropBox`, `/Rotate`) avant de changer de parent.
- `Document::move_pages` : passe lopdf (session chiffrée possible) puis `reload_from_bytes`.
  La page reste **le même objet** : signets, destinations, liens, `/Annots`, `/StructParents`
  intacts ; ADR-0007 mis à jour (la limite « signets perdus » disparaît).
- `cleanup::neutralize_dead_destinations(doc)` : après `delete_pages`, signets et liens dont la
  page n'est plus dans l'arbre perdent `/Dest` ou `/A` (signet conservé sans cible, comme Acrobat).
- Tests (`[E1d]`, `[E20a]`, `[E20c]`) : déplacer la page 1 en dernier conserve 3 liens, 4
  annotations, `BDC`, `/StructParents`, et les signets suivent ; `qpdf --check` sans alerte sur
  le widget ; `delete_pages` de la page visée : signet sans cible, aucune page fantôme.

### A6. MCID orphelins (E5)

- `cleanup::prune_orphan_mcids(doc)` sur les documents à `/StructTreeRoot` : par page, MCID
  présents dans le contenu ; `/ParentTree` → éléments → retrait de `/K` des entiers et `/MCR`
  absents. Pile explicite, profondeur bornée (ADR-0017). Test (`[E20b]`) : après
  `delete_object` sur `two_marked_objects_pdf` avec arbre, `/K` ne cite plus le MCID 0.

## B. Texte extrait et annotations tierces

### B1. Caractères hors du plan de base (L1)

- `text::drawn_chars` lit `unicode_value()` (u32) et apparie une moitié haute `D800–DBFF` avec
  la moitié basse `DC00–DFFF` qui suit (même objet texte) en un seul `DrawnChar` dont le
  rectangle est l'union des deux ; une moitié isolée est écartée comme aujourd'hui.
- Tests (`[L19-bug1937438_from_word]`) : fixture `testing::astral_text_pdf()` (DejaVu ou
  Helvetica ne suffisent pas : police Type0 avec un glyphe pour U+1D70B via `/ToUnicode`) →
  `page_text` contient `𝜋`, `search_page("𝜋")` = 1, copie identique ; test de propriété :
  aucun `TextBox` dont `ch` est une moitié de substitution.

### B2. Espaces de mots en écriture de droite à gauche (L3)

- `reading_order::insert_word_gaps(line: &mut Vec<Char>, size)` : dans une ligne, deux caractères
  consécutifs séparés d'un écart supérieur à 0,25 × hauteur reçoivent un espace synthétique
  (rectangle du blanc), **uniquement** quand la ligne ne contient aucun espace réel (ce que PDFium
  fait déjà pour le sens gauche-droite). Test pur sur des rectangles (`reading_order.rs`), puis
  `[L19-ArabicCIDTrueType]` : `page_text` contient des espaces, `search_page("الخطوط")` trouve 4.

### B3. Police CFF CID en OpenType (L2)

- Diagnostic d'abord : fixture `testing::opentype_cff_cid_pdf()` (programme CFF minimal dans
  `/FontFile3 /Subtype /OpenType`, un glyphe) ; mesurer `text_boxes` et `loose_bounds`.
- Si PDFium rend sans exposer de caractères : repli `text::tounicode_fallback(page)` par lopdf
  (lecture de `/ToUnicode` et des chaînes `Tj`/`TJ` de la page, positions approchées par la
  matrice de texte) utilisé seulement quand `drawn_chars` est vide alors que le contenu a du
  texte ; sinon correction ciblée. Décision tranchée par la mesure, consignée dans la note.
- Test : `page_text` de la fixture non vide et égal à la chaîne attendue ; corpus
  `Embedded_font.pdf` (test sauté sans `MEGA_CORPUS`).

### B4. Annotations tierces (L11)

- `read_annotation` : `editable = false` pour `FreeText` (reste affichée comme `Text`) ;
  `update_annotation`/`move_annotation` refusent avec `InvalidOperation` une annotation non
  éditable (déjà le cas pour `Other`).
- `Popup` : retirée de la liste `annotations()` (son `index` reste l'index `/Annots` des autres).
- `attachments()` : ajoute les `/FileAttachment` de chaque page (`/FS` → `/UF` puis `/F`,
  taille via `/EF /F /Params /Size` ou longueur du flux) ; `Attachment` gagne
  `page: Option<u16>` ; `save_attachment` lit le flux par `crypt::stream_bytes` ;
  `delete_attachment` d'une pièce jointe de page supprime l'annotation.
- `api.ts` : `Attachment.page`, interface : « (page N) » dans la liste des pièces jointes
  (`strings.ts`, fr et en). Tests (`[L15]`) : `third_party_annots_pdf` → `FreeText` non éditable,
  `Popup` absente, pièce jointe listée avec sa page et extraite.

### B5. Titre depuis le XMP, `/UserUnit` (L12, L13)

- `properties()` : quand `/Info` ne donne pas la valeur, repli sur `dc:title`, `dc:creator`,
  `dc:description`, `pdf:Keywords`, `xmp:CreateDate` (lecture par `xmp_value`, déjà là).
- `DocumentProperties.user_unit: Option<f32>` lu par lopdf (`/UserUnit` ≠ 1) ; `api.ts` ; la
  boîte Propriétés affiche la taille physique (« 2 100 × 2 970 mm, UserUnit 10 »). Tests
  (`[L17]`, `[L12]`).

## C. Ajouts respectueux de PDF/A et PDF/UA

- C1 (E6) : `overlay` enveloppe filigranes, numéros et couche OCR dans
  `/Artifact BMC … EMC` ; `Type0Writer` écrit `/CIDSet` ; quand l'ExtGState pose une opacité, la
  page reçoit `/Group << /S /Transparency /CS /DeviceRGB >>` s'il n'y en a pas. Tests : clés
  présentes ; veraPDF `PASS` conservé sur `pdfua-1-*` et `pdfa-2b-*` après filigrane opaque
  (sautés sans `MEGA_VERAPDF`/corpus).
- C2 (E6) : `set_metadata` écrit `dc:title`, `dc:creator`, `dc:description`, `pdf:Keywords` dans
  le XMP **même absents** (création de l'élément dans le paquet existant ; pas de création de
  paquet). Test : `pdfa-1b` reste `PASS` après `set_metadata`.
- C3 (E7) : `cleanup::drop_need_appearances(doc)` : retire `/NeedAppearances` quand tous les
  widgets ont un `/AP /N`. Test (`[E3]`) : absent dans les quatre variantes.
- C4 (E11) : `rewritten_bytes` fait, avant `save_to_bytes`, le rendu 8 px des pages qui ont un
  marquage sans objets (`annotations/mod.rs:394`) pour que `/Highlight`, `Underline`,
  `StrikeOut`, `Squiggly` aient un `/AP`. Test (`[E10]`) : `/AP /N` présent, `/BM /Multiply`.
- C5 (E8) : `protect` relève l'en-tête à 1.7 si inférieur et pose
  `/Extensions << /ADBE << /BaseVersion /1.7 /ExtensionLevel 8 >> >>`. Test (`[E7]`).
- C6 (E14) : `SigIssue::CertifiedThenModified` (`/DocMDP /P 1` et révision ultérieure) et
  `SigIssue::DocumentTimestamp` (`ETSI.RFC3161`, présenté comme horodatage) ; `strings.ts`
  `sigIssues` fr et en ; `api.ts`. Tests (`[L18]`).

## D. Taille des fichiers

- D1 (E9) : `content/image.rs` écrit l'image par lopdf (recette de `raster.rs`) : JPEG →
  XObject `/DCTDecode` avec les octets d'origine (`/DeviceRGB` ou `/DeviceGray` selon les
  composantes, CMJN refusé comme aujourd'hui) ; PNG → Flate RGB, `/SMask` seulement si un pixel a
  alpha < 255 ; flux `q cm /Im Do Q` avec ressources propres à la page (ADR-0010), espace affiché
  → utilisateur par `apply_matrix`. `InsertImage` reste un `EditCommand`. Tests (`[E8]`) :
  taille de sortie < 1,2 × JPEG, pas de `/SMask` sur un JPEG, `/SMask` sur un PNG alpha,
  `qpdf --check`, rendu identique (`compare`).
- D2 (E10) : `cleanup::subset_embedded_fonts(doc)` : pour chaque `/FontFile2` égal à
  `DEJAVU_SANS` (par taille et empreinte), GID utilisés collectés dans les contenus de page et
  les `/AP` (`Tj`/`TJ` des polices `Type0` qui le référencent, Identity-H), `subsetter` avec
  remappage identité, étiquette `XXXXXX+DejaVuSans` (`fonts::unicode::subset_tag`). Test
  (`[E6]`) : `/FontFile2` < 40 Ko, texte toujours extractible, rendu identique.

## E. Conformance et documentation

- E1 : `tests/conformance.rs` couvre les 20 commandes listées au §5 du rapport, plus un test de
  **conservation du catalogue** (`rich_catalog_pdf`, 17 structures) après chaque variante
  d'`EditCommand` ; relecture `mutool info` et `pdfjs-check.mjs` optionnelles (`MEGA_MUTOOL`,
  `MEGA_PDFJS_CHECK`, sautées si absentes) ; tests sur `fixtures/corpus/` sautés sans
  `MEGA_CORPUS`.
- E2 : quand qpdf manque et que `MEGA_REQUIRE_QPDF` n'est pas posé, le test imprime un
  avertissement **visible** (`eprintln!` en tête, et un `#[ignore]` dynamique n'existant pas, un
  message en majuscules) ; `CONTRIBUTING.md` documente `MEGA_QPDF`, `MEGA_MUTOOL`,
  `MEGA_PDFJS_CHECK`, `MEGA_VERAPDF`, `MEGA_CORPUS`.
- E3 : ADR-0007, 0008, 0009, 0010, 0011, 0013, 0018 mis à jour (§6 du rapport) ; nouvel
  **ADR-0019 « Fidélité à l'enregistrement »** : passe de report du catalogue, déplacement par
  lopdf, garde `can_clean`, version d'en-tête.
- E4 : `docs/FONCTIONNALITES.md` et aide : L7 (valeurs de champs hors recherche), L9, L10,
  L13, E12 à E15.
- E5 : les générateurs du banc migrent dans `testing.rs` ; `tests/audit_compat.rs` supprimé.

## API publiques ajoutées ou modifiées (`mega-core`)

| Élément | Changement |
|---|---|
| `cleanup::before_save(bytes, password, min_version)` | troisième paramètre `Option<(u8, u8)>` (CLI mis à jour) |
| `cleanup::can_clean`, `neutralize_dead_destinations`, `prune_orphan_mcids`, `drop_need_appearances`, `subset_embedded_fonts` | `pub(crate)`, appelées par `before_save` ou les commandes |
| `crypt::header_version(bytes) -> Option<(u8, u8)>` | pure, `pub(crate)` |
| `crypt::LopdfSession` | mode passe-plat `Identity` |
| `pages::carry::*`, `pages::reorder_kids` | pures, `pub(crate)` |
| `Document::move_pages` | même signature, lopdf + rechargement |
| `Attachment.page: Option<u16>` | nouveau champ (`api.ts`) |
| `DocumentProperties.user_unit: Option<f32>` | nouveau champ (`api.ts`) |
| `SigIssue::CertifiedThenModified`, `SigIssue::DocumentTimestamp` | nouvelles variantes (`strings.ts` fr/en, `api.ts`) |
| `text::drawn_chars` | paires de substitution ; `reading_order::insert_word_gaps` |

Commandes Tauri : aucune nouvelle (les champs transitent par les commandes existantes).

## Concepts Rust travaillés (niveau : Lot 13 acquis — `Result`, enums, pile explicite, `catch_unwind`)

- **Clone récursif avec table de correspondance** (`HashMap<ObjectId, ObjectId>`) : copier un
  sous-graphe d'objets d'un document à l'autre sans cycle infini (ensemble des visités) ;
  `[Apprentissage]` dans `pages/carry.rs`.
- **Paires de substitution UTF-16** et `char::from_u32` : pourquoi un `char` Rust ne peut pas
  être une moitié, et comment recomposer (`text.rs`).
- **Fonctions pures sur une structure de données** (lopdf) testées sans PDFium : séparation
  « calcul » / « effet » (`reorder_kids`, `carry_*`).
- **Entier à deux niveaux en tuple** `(u8, u8)` et comparaison lexicographique dérivée pour la
  version PDF (`Ord` sur les tuples).
- **Itérateurs** : `windows(2)`, `peekable`, `zip`, `scan` dans l'appariement des glyphes et la
  détection des écarts.
- **Sous-ensemble de police** : ce que fait `subsetter` et pourquoi garder les GID.

## Fixtures

Générateurs migrés du banc vers `testing.rs` : `rich_catalog_pdf`, `two_marked_objects_pdf`,
`identity_encrypted_note_pdf`, `third_party_annots_pdf`, `sig_subfilter_pdf`, `fonts_pdf`,
`deep_pages_pdf` ; nouveaux : `astral_text_pdf`, `opentype_cff_cid_pdf`, `rtl_gaps_pdf`,
`certified_then_modified_pdf`. Fichiers : `fixtures/audit/rich-2.0.pdf` (régénéré par
`gen-fixtures`). Corpus réel : jamais commité, tests dédiés sous `MEGA_CORPUS`.

## Checklist de validation manuelle de fin de lot

1. Fusionner deux PDF réels avec formulaire et pièces jointes (`fixtures/corpus/formulaires/160F-2019.pdf`
   + `navigation/attachment.pdf`) : les champs restent remplissables, la pièce jointe est listée,
   les signets sont là ; Edge ouvre le résultat sans avertissement.
2. Déplacer une page qui porte des liens, puis cliquer les liens et les signets.
3. Supprimer une annotation sur `fixtures/corpus/navigation/encrypted-attachment.pdf`, enregistrer,
   rouvrir : annotation absente ; `qpdf --show-object` ne retrouve pas son texte.
4. Ouvrir `local/edge-article.pdf` : copier la ligne avec l'émoji, chercher « 😀 » ; ouvrir
   `polices/ArabicCIDTrueType.pdf` : chercher un mot isolé.
5. Ouvrir `pdfa/pdfa-4-*` : rotation, enregistrement, `verapdf --flavour 0` reste `PASS`.
6. Insérer un JPEG de 500 Ko : le fichier ne grossit pas de plus de 20 %.
7. Insérer un texte « Ω » : le fichier reste sous 100 Ko.
8. Propriétés d'un fichier à `/UserUnit` : taille physique affichée.

## Vérification

`/verif` 8/8 sur chaque PR ; `MEGA_REQUIRE_QPDF=1` ; avec `MEGA_VERAPDF`, `MEGA_MUTOOL`,
`MEGA_PDFJS_CHECK` et `MEGA_CORPUS` posés localement, les tests étendus passent aussi ; relancer
`bash scripts/audit/write-bench.sh` et comparer `target/audit/ecriture.csv` à celui de l'audit
(aucune régression, veraPDF `PASS` conservé là où il l'était, plus les gains attendus).
