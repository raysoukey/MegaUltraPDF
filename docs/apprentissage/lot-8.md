# Lot 8 — OCR et signature numérique : note d'apprentissage

## Ce que le lot a construit

Un PDF numérisé devient cherchable : Tesseract reconnaît le texte de chaque page, qui est
ajouté en **couche invisible** (la page ne change pas d'aspect, la recherche et la copie
marchent). Un PDF se **signe** avec un certificat personnel PKCS#12 : la signature est
ajoutée à la fin du fichier sans toucher aux octets existants, si bien que les signatures
précédentes restent valides. Les signatures d'un fichier, y compris celles faites par
d'autres outils, sont **vérifiées** (intégrité, signataire, date, étendue). CLI :
`mega ocr`, `mega sign`, `mega signatures`. Version 0.8.0. Décisions : ADR-0013.

## Concepts Rust rencontrés

### 1. Features optionnelles et compilation conditionnelle

`ocr` et `signature` activent des dépendances optionnelles (`optional = true`, `dep:…`) et
des modules entiers (`#[cfg(feature = "signature")] pub mod digital_signature;`), jusqu'à une
variante d'`enum` (`EditCommand::OcrPage`) et ses bras de `match`.

- Fiche : [`features-cargo-et-dependances`](concepts/features-cargo-et-dependances.md), §5.

### 2. Lancer un programme et lire sa sortie (`crates/mega-core/src/ocr.rs:212`)

`std::process::Command::new(exe).arg(...).output()` : arguments passés un par un (aucun
shell, donc aucune injection possible), sortie lue comme du texte **non fiable**
(`parse_tsv`, ligne 69, saute toute ligne mal formée). `#[cfg(windows)]` et l'extension
`CommandExt::creation_flags` évitent une fenêtre de console (ligne 220).

### 3. Un même code générique pour deux types de clés (`digital_signature/cms.rs:116`)

`enum SigningKey { Rsa(Box<…>), Ec(…) }` (`mod.rs:46`) choisit le type de clé à
l'exécution ; dans chaque bras du `match`, le constructeur CMS générique
(`add_signer_info::<_, Signature>`) est instancié avec le bon type de signature. La clé RSA
est mise en **boîte** : sans `Box`, chaque valeur de l'`enum` prendrait la taille de sa plus
grande variante (plusieurs centaines d'octets).

### 4. Octets et décalages (`digital_signature/pdf.rs:581-610`)

Écrire un espace réservé de largeur fixe, le retrouver une seule fois (`find_unique`,
ligne 436), calculer les vraies valeurs, les réécrire **à la même largeur**
(`copy_from_slice` sur une tranche), signer `&out[..a]` et `&out[b..]`. Aucune magie : des
indices et des tranches, vérifiés par `checked_add` et des comparaisons.

### 5. Un état à deux champs (`document.rs:43-46`)

`file_bytes: Vec<u8>` (le fichier tel que chargé ou enregistré) et `changed: bool`. Une
première version tenait en un seul `Option<Vec<u8>>` (« intact » avec les octets, ou rien) :
la relecture a montré qu'il fallait garder les octets **même après une édition** (pour
vérifier les signatures du fichier), d'où deux champs.

### 6. La pile d'un thread (`crates/mega-cli/src/main.rs:29-37`)

Sous Windows, le thread principal n'a que 1 Mo de pile ; avec trois sous-commandes de plus,
l'analyse des arguments par `clap` (en mode debug) la dépassait : « has overflowed its
stack ». Le programme tourne maintenant dans un thread créé avec 8 Mo
(`std::thread::Builder::stack_size`), et `main` attend sa fin (`join`).

## Ce que les tests et la relecture ont trouvé

- **Une modification perdue.** Un enregistrement « intact » écrivait les octets d'origine…
  y compris après une modification faite par un appel direct (chemin du CLI). Un ancien test
  l'a attrapé ; toute méthode publique qui modifie le document appelle désormais
  `mark_changed()`.
- **Un test daté avant son certificat.** Le contrôle « la date de signature tombe dans la
  validité du certificat » a fait échouer des tests qui signaient à une date antérieure à la
  création des certificats de test : le contrôle marchait, la date du test était fausse.
- **Vérifier n'est pas certifier.** Le statut s'appelle `Intact`, pas `Valid` : sans chaîne
  de confiance ni révocation (réseau exclu), on prouve que le contenu n'a pas changé, pas
  qui a signé.
- **Vérification croisée** : pyHanko juge nos signatures valides, et nous lisons les siennes.
- **Un arbre qui se cite lui-même.** La relecture a construit un champ dont `/Kids` le
  cite deux fois : sans mémoire des nœuds visités (`HashSet<ObjectId>`, `pdf.rs`,
  `signature_fields`), la lecture faisait 2³³ appels **à l'ouverture** du fichier. Même
  leçon que les signets cycliques du Lot 2 : un PDF est un graphe, pas un arbre.
- **Décoder puis réencoder n'est pas neutre.** La crate `der` **trie** un `SET OF` en le
  lisant ; réencodés, les attributs signés d'une signature tierce non triée ne sont plus
  les octets signés, et une signature juste paraissait « fausse ». On lit maintenant les
  octets d'origine (`cms.rs`, `signed_attrs_span`, un petit parcours TLV écrit à la main).
- **Un échec qui laisse une trace.** `apply` annulait bien la modification refusée, mais
  pas le drapeau `changed` posé en tête de la méthode : l'enregistrement suivant
  réécrivait un fichier signé intact. Restaurer **tout** l'état, pas seulement le document.
- **Ce qui n'est pas signé ne s'affiche pas.** Une révision ajoutée après coup peut
  redéfinir le dictionnaire de signature (autre motif, autre date) : on n'affiche ses
  champs que s'il se trouve **avant** le trou de `/Contents`, donc dans les octets signés.

## Exercices (plus tard, à votre rythme)

1. `ocr.rs`, `lines_to_words` : pourquoi deux échelles (`sx`, `sy`) et pas une seule ? Que
   se passerait-il sur une page A4 rendue en 2480 × 3508 pixels avec une seule échelle ?
2. `pdf.rs`, `sign_bytes` : pourquoi les espaces réservés sont-ils cherchés **après**
   `base.len()` seulement ? Que pourrait-il arriver sinon ?
3. `cms.rs`, `der_length` : écrivez sur le papier l'en-tête DER d'une structure de 1 500
   octets, puis calculez ce que renvoie la fonction.
4. `edit.rs`, `undo` : tracez `apply`, `save_as`, `apply`, `undo` ; quelle valeur prend
   `changed` à chaque étape, et quels octets écrirait un nouvel `save_as` ?

## Ce qui reste ouvert

Signature visible (image), horodatage et LTV (réseau), chaîne de confiance et révocation,
signature de documents chiffrés, préservation des signatures lors d'une édition, OCR de
scripts non latins, ouverture d'un PDF depuis le Finder sous macOS.
