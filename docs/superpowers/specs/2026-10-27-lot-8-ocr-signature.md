# Lot 8 — OCR et signature numérique : spécification

Statut : approuvée par mandat (exécution autonome). Référence : conception §2 et §5
(« options désactivées par défaut »), ADR-0003 (lopdf), ADR-0007 (commandes d'édition),
ADR-0010 (espace affiché, passes lopdf), ADR-0011 (sécurité), ADR-0012 (distribution).

## Intention

Deux capacités avancées, optionnelles à la compilation :

1. **OCR** : rendre cherchable et copiable le texte d'un PDF numérisé, en ajoutant par-dessus
   l'image une **couche de texte invisible** placée mot par mot.
2. **Signature numérique cryptographique** : signer un PDF avec un certificat personnel
   (fichier PKCS#12 `.p12` / `.pfx`) et **vérifier** les signatures présentes dans un
   document (intégrité, signataire, date).

## Décisions de conception (rulings)

- **Moteur OCR : la commande `tesseract`** (installée par l'utilisateur), lancée comme
  processus externe. Aucune bibliothèque native liée, aucun modèle embarqué, aucun réseau.
  Recherche : chemin réglé par l'application, variable `MEGA_TESSERACT`, `PATH`, dossiers
  d'installation usuels (`C:\Program Files\Tesseract-OCR`, `/opt/homebrew/bin`,
  `/usr/local/bin`, `/usr/bin`). Sortie TSV (mots, boîtes, confiance). Si `tesseract` est
  absent, l'interface l'explique (texte, sans lien réseau).
- **Couche invisible** : un flux de contenu ajouté par `lopdf` (ressources propres à la
  page, comme les filigranes du Lot 5), police standard Helvetica en WinAnsi, mode de rendu
  `3 Tr` (invisible), chaque mot placé dans sa boîte en espace affiché puis converti
  (pages tournées comprises) et étiré horizontalement (`Tz`) à la largeur de la boîte,
  mesurée par PDFium avant la phase `lopdf`. Caractères hors WinAnsi : remplacés par `?`
  (limite documentée). Par défaut seules les pages **sans texte** sont traitées.
- **OCR page par page** : une `EditCommand::OcrPage { page, languages }` par page
  (historique, « modifié ») ; l'interface enchaîne les pages, affiche la progression et
  permet d'arrêter entre deux pages.
- **Signature : pile RustCrypto** (`cms` 0.2 avec `builder`, `x509-cert` 0.2, `rsa` 0.9,
  `p256` 0.13, `sha2` 0.10, lecture PKCS#12 par `p12-keystore`), génération stable et
  cohérente. CMS détaché (`/SubFilter /adbe.pkcs7.detached`), SHA-256, attributs signés
  `content-type`, `message-digest`, `signing-time` ; clés RSA (PKCS#1 v1.5) ou ECDSA P-256 ;
  chaîne de certificats du `.p12` incluse.
- **Signer = enregistrer** (pas une `EditCommand`) : la signature couvre des octets
  exacts ; elle est ajoutée par **mise à jour incrémentale** (les signatures existantes
  restent valides), écrite atomiquement dans le fichier choisi, puis le document est
  rechargé depuis ces octets. Documents chiffrés : refus explicite (hors périmètre).
- **Octets intacts** : `Document` garde les octets chargés (`pristine`) tant qu'aucune
  édition n'est appliquée ; `save` / `save_as` écrivent alors ces octets tels quels. Corrige
  un défaut existant : enregistrer un PDF signé non modifié invalidait sa signature.
- **Vérification sans magasin de confiance** : intégrité (empreinte des plages
  `/ByteRange` = attribut `message-digest`, signature des attributs par la clé publique du
  certificat), étendue (la signature couvre-t-elle tout le fichier, ou des modifications
  ont-elles été ajoutées après ?), signataire et date. **L'identité n'est pas certifiée**
  (pas de chaîne de confiance ni de révocation : cela demanderait le réseau, anti-objectif).
- **Features Cargo** : `mega-core` déclare `ocr` et `signature`, désactivées par défaut
  (la bibliothèque n'embarque ni la pile cryptographique ni le code OCR sans les demander) ;
  `mega-cli` et `mega-app` les activent par défaut (le produit livré les contient). La CI
  vérifie en plus `cargo check -p mega-core --no-default-features`.

## Révision après la relecture PDF (avant implémentation)

- **Octets du fichier** : `Document` garde les octets du fichier tel qu'il a été chargé ou
  enregistré (`file_bytes`) et un drapeau « modifié depuis » ; `signatures()` vérifie
  toujours ces octets, jamais une réécriture PDFium (qui casserait chaque `/ByteRange`).
  Revenir par annuler/rétablir à l'état enregistré rend de nouveau le document « intact ».
- **Mise à jour incrémentale** : `lopdf::IncrementalDocument` (ajout des seuls objets
  nouveaux ou modifiés, `/Prev` dans la remorque, type de xref de l'original). Fichier dont
  la table de références a dû être reconstruite (`xref_recovery`) ou illisible par `lopdf` :
  pas de mise à jour incrémentale (elle serait sans `/Prev`) ; le document est d'abord
  réécrit proprement, refusé s'il porte déjà des signatures (elles seraient invalidées).
- **Refus** : document chiffré, formulaire XFA (`/XFA`), document certifié sans
  modification (`/Perms /DocMDP` avec `P = 1`).
- **CMS** : `/SubFilter /adbe.pkcs7.detached` ; le constructeur de `cms` 0.2 ajoute lui-même
  `content-type` et `message-digest` (empreinte externe, pas de contenu encapsulé) ;
  l'attribut `signing-time` est écrit par nous, au même instant que `/M`. Taille de
  `/Contents` : mesurée par un essai à blanc, plus une marge ; échec propre si dépassée. Le
  fichier signé est revérifié avant d'être écrit.
- **Vérification** : champs `/FT /Sig` parcourus par `lopdf` (héritage de `/FT`), contrôle
  strict de `/ByteRange` (commence à 0, l'espace exclu est exactement la chaîne
  `/Contents`, plages dans le fichier), CMS lu sur sa longueur DER (le remplissage de zéros
  est ignoré), signature vérifiée sur les octets d'origine des attributs signés, algorithmes
  `rsaEncryption` / `sha256WithRSAEncryption` / `ecdsa-with-SHA256` ; SHA-1, horodatages de
  document et signatures d'usage (`/UR3`) : « non pris en charge ».
- **Couche OCR** : ajoutée **après** le contenu existant (l'ordre n'importe pas en mode 3) ;
  taille et ligne de base communes à une ligne de Tesseract ; espace explicite entre les
  mots d'une ligne (sinon « Page 2 » ne serait pas trouvé) ; mise à l'échelle d'après la
  taille réelle de l'image rendue. `page_has_text` compte aussi le texte invisible (relancer
  l'OCR est sans effet).
- **Nom de module** : `digital_signature` (le module `forms::signature` existe déjà pour la
  signature visuelle). La signature invisible ajoute un widget `/Sig` : les panneaux
  Formulaire et Annotations doivent l'ignorer proprement.

## Périmètre

Entre :
1. `mega-core::ocr` (feature `ocr`) : `find_tesseract`, `tesseract_languages`,
   `parse_tsv`, `EditCommand::OcrPage`, `Document::page_has_text`.
2. `mega-core::digital_signature` (feature `signature`) : `SigningIdentity::from_pkcs12`,
   `Document::sign_to(path, &identity, &SignOptions)`, `signatures(bytes) -> Vec<SignatureInfo>`,
   `Document::signatures()`.
3. `Document` : octets intacts (`pristine`), écrits par `save` sans réécriture PDFium.
4. CLI : `mega ocr <in> -o <out> [--lang eng] [--pages …] [--all]`,
   `mega sign <in> -o <out> --p12 <fichier> [--reason …] [--location …]` (mot de passe lu
   sur l'entrée standard ou `MEGA_P12_PASSWORD`), `mega signatures <in>`.
5. Tauri : `ocr_status` (tesseract trouvé ? langues), `signatures`, `sign_document` ;
   l'OCR passe par `apply_edit`.
6. Interface : Contenu ▸ « Reconnaître le texte (OCR)… » (langues, pages sans texte /
   toutes / courante, progression, arrêt) ; Fichier ▸ « Signer numériquement… » (fichier
   `.p12`, mot de passe, motif, lieu, puis « Signer et enregistrer sous… ») ; onglet
   « Signatures » du panneau latéral (statut par signature) ; bandeau quand un document
   signé est modifié (« les signatures seront invalidées à l'enregistrement »).

N'entre pas : signature visible (image) — la signature est invisible ; horodatage (TSA),
LTV, PAdES B-LT (réseau) ; validation de la chaîne de confiance et révocation ; signature
de documents chiffrés ; certification (`/DocMDP`) ; cartes à puce / magasins de certificats
du système ; OCR de scripts non latins dans la couche (WinAnsi) ; modèles OCR embarqués.

## API ajoutées (résumé)

- `ocr::find_tesseract(app: Option<&Path>) -> Option<PathBuf>` ; `search_candidates(...)` pur ;
  `tesseract_languages(exe) -> CoreResult<Vec<String>>` ; `parse_tsv(&str) -> Vec<OcrWord>` ;
  `OcrWord { text, left, top, width, height, confidence }` (pixels de l'image).
- `EditCommand::OcrPage { page: u16, languages: Vec<String> }` (`tesseract` trouvé par
  `find_tesseract` avec le dossier réglé par l'application).
- `signature::SigningIdentity::from_pkcs12(bytes, password) -> CoreResult<SigningIdentity>`,
  `SignOptions { reason, location, contact }`, `Document::sign_to(path, &SigningIdentity,
  &SignOptions) -> CoreResult<()>`, `signature::signatures(bytes) -> CoreResult<Vec<SignatureInfo>>`,
  `SignatureInfo { field, signer, signed_at, reason, location, sub_filter, status,
  covers_whole_document }`, `SignatureStatus::{Valid, Invalid(String), Unsupported(String)}`.
- `api.ts` : miroirs `OcrStatus`, `SignatureInfo`, commande `ocr_page`.

## Concepts Rust travaillés

`#[cfg(feature = …)]` sur des modules entiers et features en cascade entre crates ;
`std::process::Command` et lecture de sa sortie ; traits génériques de la pile RustCrypto
(`Signer`, `Digest`, `Encode`/`Decode` DER) ; manipulation d'octets par décalages
(`/ByteRange`, remplacement en place d'un espace réservé) ; `Option<Vec<u8>>` comme état
« intact / modifié ».

## Fixtures

- `fixtures/scanned.pdf` : page dont le texte n'existe que sous forme d'image (rendu de
  `simple.pdf` incrusté comme image), produite par `cargo xtask gen-fixtures`.
- `fixtures/test-signer.p12` : certificat auto-signé RSA-2048 de test, mot de passe `test`,
  produit par `openssl` (commande documentée) ; `fixtures/test-signer-ec.p12` (P-256).
- `fixtures/signed-external.pdf` : `simple.pdf` signé par un outil tiers (pyHanko), pour
  vérifier que notre vérification lit les signatures des autres et que pyHanko valide les
  nôtres (vérification croisée faite pendant le lot, notée dans la relecture).

## Checklist de validation (CDP `ocr-signature.json` + CLI)

1. `scanned.pdf` : la recherche de « Page 2 » ne trouve rien ; OCR (pages sans texte) ;
   la recherche trouve « Page 2 » sur la page 2 et le surlignage tombe sur le mot.
2. Annuler l'OCR : la recherche ne trouve plus rien.
3. Signer `simple.pdf` avec `test-signer.p12` (CLI) puis l'ouvrir : onglet Signatures,
   une signature « intègre, couvre tout le document, identité non certifiée ».
4. Modifier le document signé : bandeau d'avertissement ; enregistrer sous : la signature
   apparaît « invalide » ou « ne couvre plus le document ».
5. `mega signatures fixtures/signed-external.pdf` : signature intègre.
6. Enregistrer sans modification un PDF signé : signature toujours intègre.
