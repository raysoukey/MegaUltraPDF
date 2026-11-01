# Lot 14 — Compatibilité mesurée : ce que vous venez de voir

## Ce que le lot a construit

- Un **audit** mesuré (`docs/audit/2026-10-31-compatibilite-pdf.md`) : 137 PDF, quatre lecteurs
  indépendants, 28 constats, chacun corrigé ou assumé par écrit. Version 0.14.0.
- Ce que vous supprimez ne survit plus dans le fichier : si le nettoyage est impossible, la
  suppression ou l'enregistrement est refusé, et l'original reste intact.
- Fusionner, extraire ou scinder garde formulaire, signets, pièces jointes et calques ; déplacer
  une page garde ses liens ; un PDF 2.0, PDF/A ou PDF/UA le reste après une édition.
- Les fichiers sont plus légers (police Unicode réduite à moins de 40 Kio, JPEG insérés tels
  quels) ; titre et auteur survivent dans les fichiers chiffrés ou à XMP compressé.
- Côté lecture : texte arabe et polices OpenType mieux extraits, pièces jointes de page
  listées, taille physique (`/UserUnit`), verdicts « certifié puis modifié » et « horodatage ».

## Concepts Rust rencontrés

1. **`#[must_use]` et un passage obligé** : `Saved` porte `#[must_use]`
   (`crates/mega-core/src/cleanup/mod.rs:37`).
   - Le compilateur avertit si une valeur est calculée puis jetée : un nettoyage dont on
     oublierait de lire le résultat ne passerait pas `clippy -D warnings`.
   - Les octets ne sortent que par `into_checked` (`:53`), qui refuse si une suppression n'a
     pas été nettoyée ; voir l'appel dans `crates/mega-core/src/edit.rs:869`.
   - Pas encore de fiche : proposer `/concept must-use`.
2. **Une `enum` à états nommés au lieu d'`Option<Option<_>>`** : `CidMap`
   (`crates/mega-core/src/cleanup/subset.rs:62`) et `FontState` (`:596`).
   - `Some(None)` oblige à se souvenir de ce que veut dire chaque niveau ; `Unknown`, `Other`,
     `Candidate(id)` se lisent sans mode d'emploi, et `match` oblige à traiter chaque cas.
   - Fiche : [enums et match exhaustif](concepts/enums-et-match-exhaustif.md).
3. **`match` exhaustif sans bras `_`** : un oubli devient une erreur de compilation.
   - Les entrées de la table de références sont toutes nommées
     (`crates/mega-core/src/crypt.rs:816`) : si `lopdf` ajoute une variante, ce code ne
     compile plus tant qu'on n'a pas décidé quoi en faire.
   - Dans les tests, `variant` (`crates/mega-core/tests/conformance/main.rs:483`) nomme
     chaque `EditCommand`, et `missing_variants` (`:528`) vérifie à l'exécution que chacune
     est exercée.
   - Fiche : [enums et match exhaustif](concepts/enums-et-match-exhaustif.md).
4. **Lire un format binaire avec `get` et `?`** : `jpeg_header`
   (`crates/mega-core/src/content/image.rs:179`) saute de segment en segment dans un JPEG.
   - `bytes[i]` paniquerait sur un fichier tronqué ; `bytes.get(i)?` rend `None` et arrête
     proprement la lecture.
   - Fiche : [`Result` et l'opérateur `?`](concepts/result-et-operateur-point-d-interrogation.md)
     (le `?` marche de la même façon sur une `Option`).
5. **Un bloc `{ … }` qui borne la vie d'un verrou** : dans `to_bytes`
   (`crates/mega-core/src/document.rs:215`), le jeton `_guard` est libéré à la fin du bloc,
   avant `seal_metadata` qui n'a pas besoin de PDFium.
   - Fiches : [Mutex et état partagé](concepts/mutex-et-etat-partage.md),
     [`Drop` et ressources natives](concepts/drop-et-ressources-natives.md).
6. **Un cache `OnceLock` dans une structure** : `lopdf_cache`
   (`crates/mega-core/src/document.rs:74`).
   - Rempli une seule fois par `get_or_init` (`:273`) à travers `&self`.
   - Vidé par `take` (`:252`), qui demande `&mut self` : personne ne peut le lire à ce moment.
   - Fiche : [`OnceLock`](concepts/oncelock.md).
7. **Un pointeur de fonction dans une `static`** : `static SINK: OnceLock<fn(&str)>`
   (`crates/mega-core/src/diagnostic.rs:21`).
   - `mega-core` ne connaît pas le journal de l'application ; l'application lui donne une
     fonction au démarrage (`app/src-tauri/src/diagnostic.rs:121`), que `warn` appelle (`:31`).
   - `fn(&str)` ne capture rien (contrairement à une fermeture) : il tient dans une `static`
     sans `Box`.
   - Fiche : [`OnceLock`](concepts/oncelock.md).
8. **Un objet-trait choisi à l'exécution** : `MetadataCipher::Encrypt(Arc<dyn CryptFilter>)`
   (`crates/mega-core/src/crypt.rs:490`).
   - Le fichier dit lequel de RC4, AES-128 ou AES-256 employer ; les appels vont à
     l'implémentation de ce type-là. `Arc` partage le filtre sans le copier.
   - Fiche : [traits et objets de trait](concepts/traits-et-objets-de-trait.md).
9. **Pile explicite, encore** (suite du Lot 13) :
   - `collect_usage` range les contenus à lire dans un `Vec` (`cleanup/subset.rs:296`, boucle
     `:328`) ;
   - `visit_references` empile des `&mut Object` (`crates/mega-core/src/pages/carry.rs:176`) :
     chaque emprunt dépilé est consommé, deux `&mut` ne visent jamais le même objet ;
   - `text/tounicode.rs:35` (un cadre par formulaire) et `cleanup/mcid.rs:180`.
   - Fiche : [complexité et collections](concepts/complexite-et-collections.md).
10. **Une recherche en temps linéaire sur des octets hostiles** : `may_have_filtered_metadata`
    (`crates/mega-core/src/crypt.rs:719`).
    - Trois curseurs n'avancent que vers la fin du fichier : aucune recherche ne repart de
      zéro pour chaque nom trouvé. Le test `:1361` le prouve sur 1 Mio de noms.
    - Fiche : [complexité et collections](concepts/complexite-et-collections.md).
11. **Une fonction pure sur un `lopdf::Document`** : `reorder_kids`
    (`crates/mega-core/src/pages/reorder.rs:232`) ne prend ni fichier ni PDFium.
    - Ses tests (`:314`) construisent un petit document à la main et regardent le résultat.
    - Même idée pour `load_preferring` (`crates/mega-core/src/document.rs:441`) : générique,
      elle reçoit le chargement sous forme de fermeture, et le test lui en passe une qui
      simule un refus.
    - Pas encore de fiche : proposer `/concept fonctions-pures-et-tests`.
12. **Un canal avec délai au lieu de `join`** : `run_with_timeout`
    (`crates/mega-core/tests/conformance/main.rs:132`).
    - Deux fils vident les sorties d'un programme externe et envoient le résultat par un
      canal `mpsc` (`:142`) ; le fil principal attend avec `recv_timeout` (`:175`).
    - `join` attendrait pour toujours si un petit-enfant garde le tuyau ouvert.
    - Fiche : [canaux et patron acteur](concepts/canaux-et-acteur.md).

## Décisions de conception

- **ADR-0019**, fidélité à l'enregistrement (`docs/adr/0019-save-fidelity.md`) :
  - pas de repli silencieux après une suppression : garde `ensure_cleanable`
    (`crates/mega-core/src/document.rs:295`) **avant** `mark_changed`, puis `into_checked` ;
  - passe-plat pour les filtres `Identity` : élagué sans renuméroter, car le numéro d'un objet
    entre dans la clé de ses octets chiffrés ;
  - version d'en-tête d'origine reposée après PDFium (`crypt::header_version`), jamais baissée ;
  - report du catalogue après fusion, extraction, scission (`pages/carry`), sans scripts ni
    arbre de structure ni `/MarkInfo` ;
  - déplacer = réordonner `/Kids`, la page reste le même objet ;
  - MCID orphelins retirés seulement quand leur absence est prouvée ;
  - le XMP réparé là où les octets passent entre PDFium et nous : à la sortie (chiffrement), à
    l'entrée (compression) ;
  - ce que nous ajoutons suit la déclaration PDF/A ou PDF/UA lue dans le XMP ;
  - police réduite seulement dans les octets écrits ; **tout doute laisse la police entière** ;
  - chaque variante d'`EditCommand` passe `qpdf --check`, sans erreur ni avertissement.
- **ADR mis à jour** :
  - 0008 : apparence écrite pour chaque marquage (E11) ;
  - 0009 : `/NeedAppearances` retiré après régénération, formulaire reporté à la fusion ;
  - 0010 : filigrane et numéros balisés comme artefacts ; suppression refusée si `lopdf` ne
    relit pas le fichier ;
  - 0011 : métadonnées reportées dans le XMP, `/Extensions` pour AES-256 ;
  - 0013 : « certifié puis modifié » (`/DocMDP P 1`) et horodatage de document (E14) ;
  - 0014 : police Unicode réduite à l'enregistrement ;
  - 0018 : mesures veraPDF avant et après, `tests/conformance/` devenu un dossier.

## Pièges rencontrés

- **Un test « instable » qui cachait une vraie perte** : la relecture par MuPDF échouait au
  hasard. Cause : dans un document chiffré, PDFium écrit le XMP en clair, puis le « déchiffre »
  en relisant sa propre sortie ; on obtenait des octets aléatoires, que l'analyseur XML de
  MuPDF acceptait ou non selon le tirage.
  - Premier contournement (`encrypt_metadata: false` à la protection) annulé : il ne réglait
    que nos propres fichiers.
  - Correction : `seal_metadata` rechiffre ce seul flux à chaque sortie de PDFium
    (`crates/mega-core/src/document.rs:219`, `crypt.rs:662`).
- **PDFium retire le `/Filter` d'un XMP compressé** mais garde les octets compressés : titre,
  auteur et déclaration PDF/A illisibles dès la première édition. On lui donne une copie au XMP
  décodé, par un seul point d'entrée (`load_file_bytes`, `document.rs:423`). Le rechargement
  après une signature contournait d'abord la correction : d'où ce point unique.
- **Une lecture `lopdf` trop indulgente** : `lopdf` saute un objet mal formé sans erreur. La
  copie réécrite pouvait donc montrer à PDFium un **autre** document. Elle n'est faite que si
  tout concorde (`unfiltered_metadata_copy`, `crypt.rs:793`) :
  - `%PDF-` au tout début, lecture sans réparation de la remorque ;
  - table de références non réparée ;
  - chaque entrée en usage présente (`:810`) ;
  - même nombre de pages après réécriture (`:853`).
- **Un balayage quadratique** : pour chaque `/Metadata` trouvé, la première version de
  `may_have_filtered_metadata` cherchait le `obj` précédent et le `stream` suivant à travers
  tout le fichier ; 1 Mio de `/Type/Metadata` répétés prenait des minutes. Des curseurs qui
  n'avancent jamais en arrière, et une fenêtre de 4 Kio, ont réglé cela (`crypt.rs:719`).
- **`mark_changed()` appelé avant les gardes** : un appel refusé laissait le document
  « modifié », et l'enregistrement suivant le réécrivait au lieu de garder ses octets
  d'origine (une signature aurait été rompue).
  - Corrigé pour `protect`, `unprotect`, le filigrane, les numéros de page, la couche OCR et
    le déplacement.
  - `crop_pages` validait page par page en recadrant : un refus sur la page 3 laissait les
    pages 1 et 2 recadrées. Il calcule maintenant tout d'abord
    (`crates/mega-core/src/overlay.rs:414`).
- **Le trou `ExtGState /Font` du sous-ensemble de police** : l'opérateur `gs` peut changer de
  police sans `Tf`, et les glyphes suivants auraient été comptés pour l'ancienne police. La
  passe renonce alors (`graphics_state_escapes`, `cleanup/subset.rs:444`). Même revue : un
  fichier citant les 65 536 CID faisait paniquer `subsetter` (`:794`).
- **Pas de `FPDF_MovePages` dans pdfium-render** : l'ancien déplacement copiait la page, nouvel
  objet que plus rien ne visait. On réordonne `/Kids`, et l'on refuse si `lopdf` ne voit pas
  les mêmes pages que PDFium (`crates/mega-core/src/pages/mod.rs:141`).
- **L'éditeur de liens à court de mémoire** : la suite de conformité se lance avec `-j 2`, deux
  compilations à la fois au lieu d'une par cœur (`CONTRIBUTING.md:49`).

## Exercices de lecture

1. `crates/mega-core/src/pages/mod.rs:116` (`move_pages`) : pourquoi `reordered_bytes` est-il
   appelé **avant** `mark_changed`, et que perdrait un fichier signé si l'ordre était inverse ?
2. `crates/mega-core/src/crypt.rs:793` (`unfiltered_metadata_copy`) : pourquoi les octets
   d'origine (`file_bytes`) ne sont-ils jamais remplacés par la copie, alors que PDFium ne voit
   que la copie ?
3. `crates/mega-core/src/cleanup/subset.rs:617` (`read_strings`) : que se passerait-il si
   l'on oubliait de dépiler `saved` sur un `Q` ? Quels glyphes seraient comptés pour la mauvaise
   police ?

## Prérequis pour la suite

- Relire [`enums et match exhaustif`](concepts/enums-et-match-exhaustif.md) : le lot s'en sert
  comme garde-fou contre les oublis.
- Savoir lire la sortie de `qpdf --check` et d'un lecteur indépendant : `CONTRIBUTING.md` décrit
  les variables `MEGA_*` de la suite de conformité.
- Pour les dettes de l'ADR-0019 (greffe du XMP en place, signatures certifiées `P = 2` ou `3`) :
  notions de table de références en flux et de révisions incrémentales d'un PDF.
