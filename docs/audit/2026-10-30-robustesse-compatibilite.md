# Audit de robustesse et de compatibilité (Lot 13, 2026-10-30)

Audit en lecture seule du code de la version 0.12.2, puis correction de **tous** les risques
trouvés (décision de l'utilisateur). Chaque ligne donne la preuve (`fichier:ligne` dans la
0.12.2), la correction et son statut, mis à jour à chaque PR du lot.

Gravité : **É** élevée (perte de données, application figée ou fermée, fichier illisible
ailleurs), **M** moyenne (fonction fausse ou inutilisable sur une classe de fichiers),
**F** faible (cas rare, message peu clair).

## Ce qui était déjà solide

- Parcours de graphes bornés : signets (`outline.rs:67`, profondeur 64, 10 000 nœuds), champs
  de signature (`digital_signature/pdf.rs:168`, ensemble des nœuds vus), élagage
  (`cleanup.rs:79`, liste de travail), chaînes `/IRT` et `/Parent` (bornées).
- Rendu plafonné à 50 Mpx (`render.rs:77`), échelle bornée côté Tauri (`protocol.rs:24`).
- Enregistrement : replis sûrs de `cleanup::before_save` (octets PDFium si lopdf échoue ou si
  la relecture diffère), copie octet pour octet d'un fichier non modifié, écriture atomique.
- Chiffrement réécrit à l'identique (RC4-128, AES-128, AES-256 R6, propriétaire seul) ;
  filtres inhabituels refusés.
- Historique plafonné (20 étapes, 256 Mo), cache d'images plafonné (256 Mo).

## Robustesse (fichiers malformés ou hostiles, ressources)

| # | G | Risque | Preuve (0.12.2) | Correction | Statut |
|---|---|---|---|---|---|
| R1 | É | Bombe de décompression : un flux minuscule qui se décompresse en gigaoctets, lu sans limite à chaque enregistrement et par le contenu, le caviardage, les métadonnées | `cleanup.rs:69`, `content/mod.rs:216`, `raster.rs:560`, `cleanup.rs:570`, `overlay.rs:730`, `properties.rs:334` ; aucune `LoadOptions::max_decompressed_size` | Lecture lopdf unique `crypt::load_limited`, `get_page_content_with_limit`, limite par flux | ✅ corrigé (PR A) |
| R2 | É | Un `tesseract` bloqué fige l'acteur unique : plus aucun rendu ni document | `ocr.rs:400` (`.output()` sans délai) | Délai par page, processus tué, erreur typée | ✅ corrigé (PR A) |
| R3 | M | Chaîne profonde de XObjects imbriqués : débordement de pile, l'application se ferme | `content/raster.rs:404` ↔ `:491`, récursion sans profondeur maximale | Pile explicite, profondeur bornée, réponse prudente | ✅ corrigé (PR A) |
| R4 | M | Une panique dans l'acteur l'arrête pour de bon : tout renvoie « service arrêté » jusqu'au redémarrage | `service/mod.rs:341-366`, pas de `catch_unwind` | `catch_unwind` par message, document fautif fermé | ✅ corrigé (PR A) |
| R5 | M | Erreur de rendu invisible : page blanche sans explication | `PageView.svelte:53` | Cadre d'erreur avec message | ✅ corrigé (PR A) |
| R6 | M | Aucune limite de taille de fichier ; trois chargements lopdf par enregistrement | `document.rs:81`, `cleanup.rs:29/164/58` | Refus au-delà de 2 Gio, un chargement de vérification au lieu de deux | ✅ corrigé (PR A) |
| R7 | M | JPEG décodé avant le contrôle de ses dimensions | `raster.rs:237`, `optimize.rs:150` | `image::Limits` avant décodage | ✅ corrigé (PR A) |
| R8 | F | Fichier endommagé (xref cassée) : liste des signatures en erreur, permissions « refusées » en silence | `digital_signature/pdf.rs:383`, `security.rs:197`, `properties.rs:173` | État « structure endommagée », repli sur PDFium | ✅ corrigé (PR A) |
| R9 | F | Aucun test de fichier malformé ni de fuzzing | `fixtures/` : 16 PDF valides | Corpus `fixtures/robustesse/`, `cargo xtask fuzz`, tests de propriétés | ✅ corrigé (PR A) |
| R10 | F | Aucun journal pour comprendre une panne chez l'utilisateur | pas de journal ; `eprintln!` seulement (`service/mod.rs:345`) | Journal local plafonné, sans contenu de document | ✅ corrigé (PR A) |

## Compatibilité des fichiers PDF

| # | G | Risque | Preuve (0.12.2) | Correction | Statut |
|---|---|---|---|---|---|
| C1 | É | Aucun contrôle que nos sorties s'ouvrent ailleurs | ni qpdf, ni pdfinfo, ni mutool dans les tests ou la CI | `qpdf --check` sur la sortie de chaque édition, obligatoire en CI | PR B |
| C2 | É | Signature d'un fichier à xref hybride ou à révisions multiples : mise à jour incrémentale calculée sur la vue de lopdf | `digital_signature/pdf.rs:894-913` | Vue lopdf comparée à PDFium, réécriture ou refus ; fixtures dédiées | PR B |
| C3 | M | Texte de droite à gauche (hébreu, arabe) copié à l'envers | `reading_order.rs:142-162` | Sens de lecture par ligne | PR B |
| C4 | M | Écriture verticale (CJK) découpée en lignes d'un caractère | `text.rs:174-204` (seul le texte tourné est traité) | Suites verticales gardées dans l'ordre de dessin | PR B |
| C5 | M | La recherche suit l'ordre de PDFium, la sélection l'ordre de lecture | `text.rs:150-154` | Recherche dans le texte en ordre de lecture | PR B |
| C6 | M | Déclaration PDF/A ni signalée ni protégée après modification | `optimize.rs:291` seulement | Détection, test de conservation, avertissement | PR B |
| C7 | F | Caractères sans Unicode (Type3, CID sans `/ToUnicode`) écartés : trous dans la sélection | `text.rs:114` | Gardés en `U+FFFD` | PR B |
| C8 | F | RC4-40 et AES-256 R5 jamais testés | `tests/encrypted.rs` | Fixtures qpdf | PR B |
| C9 | F | Fichier chiffré par certificat (`/Adobe.PubSec`) : message de mot de passe trompeur | — | Erreur typée « chiffrement non pris en charge » | PR B |
| C10 | F | Mots de passe limités à U+00FF | `security.rs:8-10` | SASLprep + UTF-8 (R6) | PR B |

## Plateforme et interface

| # | G | Risque | Preuve (0.12.2) | Correction | Statut |
|---|---|---|---|---|---|
| P1 | É | Réordonner les miniatures par glisser-déposer HTML5 : intercepté par le glisser-déposer natif de Tauri sous Windows | `Thumbnails.svelte:146/235`, aucun `dragDropEnabled` | Événements pointeur | PR C |
| P2 | É | WebView2 téléchargé à l'installation (échec hors ligne) ; macOS Apple Silicon seulement ; pas de version minimale macOS | `tauri.conf.json`, `release.yml:68-75` | Amorce WebView2 embarquée, `.dmg` universel, `minimumSystemVersion` | PR C |
| P3 | M | Pages floues sur écran à 150 % ou 200 % | `render.rs:77`, `devicePixelRatio` jamais lu | Rendu à l'échelle × DPR | PR C |
| P4 | M | Enregistrement : message générique pour un fichier en lecture seule, verrouillé ou un dossier interdit ; permissions et liens physiques perdus | `edit.rs:851-881`, `error.rs:62` | Erreurs typées, permissions recopiées, liens physiques préservés (Unix) | PR C |
| P5 | F | Échec du presse-papiers silencieux | `+page.svelte:198` | Repli et message | PR C |
| P6 | F | Chemins non UTF-8, longs ou accentués non testés | `commands.rs:181/595` | `PathBuf` côté Rust, tests | PR C |
| P7 | F | `.deb` sans dépendances explicites | `tauri.conf.json` | `depends`, `recommends: tesseract-ocr` | PR C |

## Mesures de la PR A

- `tests/robustness.rs` : les 13 fichiers du corpus `fixtures/robustesse/` passent la chaîne
  complète (ouvrir, rendre, texte, recherche, filigrane, caviardage, optimisation,
  enregistrement, relecture). Avant correction, `xobjects-profonds.pdf` tuait le processus
  (`STATUS_STACK_OVERFLOW`) et la bombe était décompressée à chaque enregistrement.
- `cargo xtask fuzz --iterations 20000` (graines 0 à 19 999, fixtures et corpus) : 7 774
  fichiers abîmés encore ouverts par PDFium, **aucune panique ni plantage**, 146 s.
- Un cas trouvé par sonde : remorque sans `/Root` d'un fichier signé, signature absente de
  la liste (R8, corrigé).

## Limites assumées

- Sous Windows, les ACL explicites et les flux de données alternatifs d'un fichier remplacé
  sont perdus au renommage atomique ; les garder exigerait `ReplaceFileW`, donc du code
  `unsafe`, interdit dans ce projet (voir ADR-0007).
- Un plantage **natif** de PDFium ferme le processus : il n'y a pas d'isolation par processus.
