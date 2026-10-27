# Lot 9 — Consolidation : spécification

Date : 2026-10-27 · Prérequis : Lots 0 à 8 fusionnés (v0.8.0).

## Intention

Chaque lot s'est terminé par une relecture indépendante ; les constats **mineurs** ont été
consignés et reportés (« minor (deferred) » des registres archivés `%TEMP%\lot-N-ledger-final.md`).
Ce lot les reprend. Un tri en lecture seule de `main` (971bcb6) a classé 110 points :
22 déjà corrigés par les lots suivants, 1 obsolète (note de processus), environ 80 encore
ouverts ou partiels. Ce lot corrige ceux qui tiennent en une correction locale (effort S ou
M), chacun **avec un test vu échouer** (Rust ou Vitest) ou une étape de scénario CDP, et
consigne explicitement ceux qui demandent une vraie nouvelle fonctionnalité (effort L).

Aucune nouvelle fonctionnalité visible n'est ajoutée, sauf trois qui comblent un manque
signalé : redimensionner une image insérée (B31), choisir le chemin de Tesseract dans les
préférences (C41), ouvrir un PDF depuis le Finder sous macOS (C33).

Identifiants : **A** = Lots 0-2, **B** = Lots 3-5, **C** = Lots 6-8 (numérotation du tri).

## Décisions (rulings)

1. **Priorité à la sécurité.** B20 (une note caviardée survit par son popup) passe en tête :
   c'est un défaut de caviardage, pas un détail. Le correctif porte sur la passe de nettoyage
   (`cleanup::before_save`) et sur la suppression d'annotation : un popup dont le parent a
   disparu est retiré, une réponse (`/IRT`) vers une annotation disparue perd ce lien, puis
   l'élagage retire la note.
2. **Hors périmètre (effort L, nouvelles fonctionnalités, lot ultérieur éventuel)** :
   - incorporation d'une police Unicode (texte hors WinAnsi des tampons, notes, champs, texte
     retouché : B2-police, B5-police, B18-police, B22-police) ; ce lot pose à la place des
     **refus explicites** là où le texte serait abîmé ;
   - caviardage raster d'une image partiellement couverte (B21) et retrait réel des chemins
     et des XObjects de formulaire (B19, limite documentée) ;
   - nettoyage des annotations d'un document **chiffré** (C20 : déchiffrer, nettoyer,
     rechiffrer) ;
   - refonte des 17 booléens `xxxOpen` de `+page.svelte` en une union et composant
     `Dropdown.svelte` (C30, parties L) — seules les parties S de C30 sont faites ;
   - OCR en **une seule** étape d'annulation pour plusieurs pages (C40, partie L) ; ligne
     de base exacte par hOCR (C39, partie baseline) ;
   - vérification visuelle de l'opacité dans Aperçu (B4) et de l'ouverture Finder (C33) sur
     un vrai Mac : impossible sur la machine de développement ; le code d'ouverture Finder
     est écrit et compilé par la CI macOS, la vérification manuelle reste à faire.
3. **Une table des largeurs Helvetica WinAnsi** (AFM, 224 entrées) sert à la fois aux
   `/Widths` des polices d'overlay (B24) et à la mesure des mots OCR (C46, `Tz` fondé sur
   l'avance et non sur l'encre) : `mega-core/src/fonts.rs` (nouveau, `pub(crate)`).
4. **XMP (C11)** : `set_metadata` met à jour, dans un flux `/Metadata` existant (décodable),
   le contenu des éléments déjà présents `dc:title`, `dc:creator`, `dc:description`,
   `pdf:Keywords`, `pdf:Producer`, `xmp:CreatorTool`, `xmp:ModifyDate` ; il n'en crée pas
   (pas de générateur XMP). Un flux illisible est laissé tel quel.
5. **`user_password_required` (C9)** devient « le fichier exige un mot de passe pour
   s'ouvrir » (vrai seulement si un mot de passe **utilisateur** non vide existe), calculé
   par un essai d'ouverture sans mot de passe.
6. **Détail des verdicts de signature (C43)** : `SignatureStatus::Invalid` / `Unsupported`
   gardent un `String` (texte français) **et** gagnent un code stable (`reason: SigIssue`,
   enum sérialisée en `snake_case`) que l'interface traduit ; le texte reste pour le CLI.
7. **Rapport d'optimisation (C21)** : présenté comme une estimation (« environ ») ; écrire
   exactement les octets optimisés demanderait de court-circuiter la réécriture PDFium.
8. **Nouvelle dépendance** : `rpassword` (C42, saisie du mot de passe PKCS#12 sans écho dans
   le CLI quand l'entrée est un terminal) — raison écrite dans le message de commit.
9. **Garde d'écrasement du CLI (B16)** : toute commande qui écrit `--out` refuse un fichier
   existant sans `--force` (comme `split`/`export` déjà), y compris l'écrasement de l'entrée.
   C'est un changement de comportement du CLI, noté dans le README.
10. Version **0.9.0**.

## Périmètre (ce qui entre)

### Sécurité et CLI
- B20 popups et `/IRT` orphelins retirés au nettoyage ; suppression d'une annotation qui
  emporte son popup.
- B16 garde `--force` ; B30 `rotate`/`flatten`/`extract` passent par `save_as` / le
  nettoyage ; B23 avertissement CLI avant de réécrire un document signé ; B32
  `mega redact --pages <spec>` ; C10 `mega compare --password-b` ; C42 mot de passe sans écho.

### Annotations (B)
- B1 `/CreationDate` conservée à la modification, `/M` mis à jour ; B3 `/NM` (unique) et
  `/P` ajoutés au nettoyage quand ils manquent ; B5 refus du texte hors WinAnsi (notes,
  tampons) ; B9 `is_finite` sur épaisseur, taille de police, géométrie ; B2 retour à la
  ligne du texte des tampons et notes libres à la largeur du cadre ; B25 `/InkList` : courbes
  de Bézier découpées en segments, `/Matrix` de l'apparence appliquée.

### Formulaires (B)
- B17 état coché lu soi-même (`/AS`, sinon `/V`, comparé à `Off`) ; B18 refus du texte hors
  WinAnsi pour un champ en police simple non incorporée ; B15 `last_image_bounds` réservé
  à la feature `testing`.

### Contenu et polices (B, C)
- B24 `/FirstChar` `/LastChar` `/Widths` ; C46 mesure OCR par avance AFM ; B26 police prise
  une fois par opération ; B22 refus de caractères nouveaux dans une police sous-ensemble ou
  incorporée ; B29 `/MediaBox` héritée lue ; B31 commande `ResizeObject` et poignées.

### Métadonnées, sécurité, pièces jointes (C)
- C11 XMP ; C12 `/ModDate` à chaque enregistrement réécrit ; C13 `/Info` direct conservé ;
  C14 caractères de contrôle refusés ; C15 lecture de `/ModDate` et de `/Encrypt` sur
  `file_bytes` quand rien n'a changé ; C16 `/ModDate` d'un fichier à mot de passe
  utilisateur ; C17 `/EFF` différent de `/StmF` refusé (« non pris en charge ») ; C9 ;
  C44 PDFDocEncoding exact et UTF-8 avec BOM ; C7 pièce jointe lue une seule fois (borne).

### Optimisation et export (C)
- C5 image réduite gardée seulement si plus petite ; C6 Flate quand aucune recompression ;
  C21 « environ » ; C22 élagage par ensemble (`BTreeSet`) au lieu de `prune_objects` ;
  C23 test de comparaison CMYK ; C18 densité (pHYs PNG, JFIF JPEG) à l'export.

### Signatures (C)
- C34 `/ByteRange` strict ; C35 `/Perms /UR3` signalé « non pris en charge » ; C36 refus
  avant réécriture d'un fichier illisible par lopdf qui contient `/ByteRange` ; C37 `/AP`
  vide du widget, `/NeedAppearances false`, second `/ID` renouvelé, `/Version /1.6` si
  l'en-tête est plus ancien ; C38 remorque nettoyée des clés de flux de références ;
  C45 `signing-certificate-v2` vérifié, `id-ecPublicKey` accepté comme ECDSA ; C43 codes.

### OCR (C)
- C40 page sans mot : ni étape d'historique ni « reconnue » ; rendu sans annotations ;
  langues vérifiées une fois par lot de pages ; échec de `pagesWithText` affiché ;
  C41 entrées vides du `PATH` ignorées, préférence « Chemin de Tesseract » ; C39 test OCR
  sur page tournée (ignoré sans Tesseract).

### Service, historique, cache (A)
- A4 refus d'insertion au-delà de 65 535 pages ; A18 `Entry` comparée sur (priorité,
  séquence), `PartialEq` factice de `Request` supprimé ; A19 `PngCache::touch` en
  O(log n), fixtures déterministes (`/ID` fixe) ; A24 pile « rétablir » bornée en octets ;
  A25 test de la valeur par défaut (20 étapes) ; A17 test d'ordonnancement sans `sleep` ;
  B11/B12 annotations et champs de **tout** le document en une requête.

### Interface (A, B, C)
- A14 zoom ancré sous le curseur ; A15 un seul `{#each}` (plus de clignotement) ; A16 molette
  en mode page unique change de page aux bords, `busy` affiché ; A22 dépôt entre deux
  vignettes au bon endroit ; A23 annuler/rétablir sérialisés ; A20 `positions` borné ;
  A13 `keyCtrl` ; B14/B27 pas de cache vide sur erreur ; C8 erreur affichée dans le dialogue
  d'export ; B10 libellés de raccourcis tirés de `shortcuts.ts` ; B6 `draftRect` remplacé ;
  C43 clé d'index et traduction des verdicts.
- Accessibilité : C24 `trapFocus` (radios, `fieldset` désactivé) ; C25 `lang` initial,
  messages effacés au changement de langue ; C26 nombres localisés, `{title}` accepté ;
  C27 détails ARIA des menus ; C28 `use:tabNav` sur les cinq listes d'onglets ; C29 chaînes
  restantes, termes harmonisés, clés inutilisées supprimées (test « toute clé est utilisée ») ;
  C30 parties S (`ZoomModeLike`, règles `:focus-visible`) ; C32 état conservé à travers
  `{#key}` ; B7 panneau des annotations, B13 panneau des formulaires ; B8 calque au-dessus
  des liens en mode sélection.

### Distribution (A, C)
- C31 `pnpm licenses --prod`, `serde_json` en dépendance d'espace de travail ; C33
  `RunEvent::Opened` (macOS) ; A10 `#[cfg(debug_assertions)]` au lieu de `cfg!`.

## API ajoutées ou modifiées (résumé)

- `mega-core` : `fonts::helvetica_width(byte) -> u16` (crate) ; `EditCommand::ResizeObject
  { page, index, rect }` ; `Document::all_annotations()` / `all_form_fields()` ;
  `SigIssue` (codes de verdict) dans `SignatureStatus` ; `ocr::OcrOutcome { words }`
  retourné par l'OCR d'une page ; `SecurityInfo::user_password_required` (sens précisé).
- Tauri : `all_annotations`, `all_form_fields`, `set_tesseract_path` ; `api.ts` aligné.
- CLI : `--force` partout où `--out` écrit ; `redact --pages` ; `compare --password-b`.

## Concepts Rust travaillés

Comparaison et ordre (`PartialEq`/`Eq`/`Ord` cohérents, `BinaryHeap`) ; `BTreeMap` /
`BTreeSet` contre `Vec::contains` (complexité) ; lecture bornée (`Read::take`) et TOCTOU ;
tables constantes (`const [u16; 224]`) ; `#[cfg]` contre `cfg!` ; parcours de graphe avec
ensemble de visités (suite du Lot 8). Fiche concept prévue : `complexite-et-collections`.

## Fixtures

Squelettes écrits à la main (`testing.rs`) : note avec popup hors zone ; annotation sans
`/AP` ; case à cocher sans `/AP` et case « On » ; `/MediaBox` héritée ; XMP non compressé ;
`/Info` direct ; `/EFF` ; `/Perms /UR3` ; page CMYK ; police sous-ensemble ; fichier signé
illisible par lopdf.

## Checklist de validation

- `/verif` 8/8 ; `cargo check -p mega-core --no-default-features`.
- Les 16 scénarios CDP passent, plus les étapes ajoutées (zoom sous le curseur, molette en
  page unique, dépôt entre vignettes, double Ctrl+Z, onglets au clavier, redimensionnement).
- Un fichier caviardé dont la note avait un popup ne contient plus le texte de la note.
- pyHanko juge toujours valides nos signatures, et `qpdf --check` passe sur le résultat.
