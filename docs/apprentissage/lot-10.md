# Lot 10 — Fonctionnalités restantes : ce que vous venez de voir

## Ce que le lot a construit

Ce qui change pour l'utilisateur :

- **Texte en grec, en cyrillique ou en latin étendu partout** : filigrane, numéros de page,
  texte inséré, retouche, texte libre, tampon, couche OCR. La police DejaVu Sans est
  incorporée au fichier.
- **Caviardage d'une partie d'image** : seuls les pixels sous la zone deviennent noirs et
  disparaissent du fichier. Les tracés dans la zone partent aussi.
- **Signature numérique visible** : un tampon avec le nom, la date, le motif et une image
  facultative. Il reste droit sur une page tournée.
- **Documents protégés par mot de passe** : on peut les filigraner, les caviarder, remplir
  leurs formulaires et modifier leurs métadonnées. Ils restent protégés à l'identique.
- **OCR de plusieurs pages annulable d'un seul Ctrl+Z.**
- Vérification des signatures SHA-384 et SHA-512, dialogues unifiés, mineurs du Lot 9.
  Version 0.10.0.

## Concepts Rust rencontrés

1. **`include_bytes!` : un fichier dans le binaire.**
   - La police est copiée dans l'exécutable à la compilation et vue comme un `&'static [u8]`.
     Aucun fichier n'est à chercher à l'exécution.
   - À lire : `crates/mega-core/src/fonts/unicode.rs:27`.
2. **`OnceLock` pour une analyse coûteuse faite une fois.**
   - `UnicodeFont::embedded` analyse les tables de la police au premier appel, puis partage le
     résultat (`crates/mega-core/src/fonts/unicode.rs:37`).
   - Fiche : [oncelock](concepts/oncelock.md).
3. **`Option::take` : sortir une valeur et laisser `None`.**
   - La session `lopdf` prend l'état de chiffrement du document pour le lui rendre au
     rechiffrement (`crates/mega-core/src/crypt.rs:51`).
   - Un seul propriétaire à la fois : impossible de l'oublier dans le document.
4. **`self` par valeur : consommer un objet.**
   - `Type0Writer::write(self, …)` (`crates/mega-core/src/fonts/unicode.rs:182`) et
     `LopdfSession::finish(mut self)` (`crates/mega-core/src/crypt.rs:56`) prennent
     l'écrivain ou la session.
   - Le compilateur refuse ensuite de s'en servir : on ne peut pas encoder un texte dans une
     police déjà écrite.
5. **État explicite avec `Option<struct>`.**
   - Un groupe d'annulation est `Option<Group>` (libellé + « déjà enregistré ? ») plutôt
     qu'un booléen et un libellé séparés (`crates/mega-core/src/edit.rs:645`, utilisé l. 606).
   - Fiche : [enums et match](concepts/enums-et-match-exhaustif.md).
6. **Fermeture en paramètre (`impl FnMut`).**
   - `cms::verify_with` demande à l'appelant *comment* obtenir une empreinte, et décide
     elle-même *laquelle* il lui faut (`crates/mega-core/src/digital_signature/cms.rs:360`).
   - Fiche : [itérateurs et fermetures](concepts/iterateurs-et-fermetures.md).
7. **Fonction générique avec une borne de trait.**
   - `fn run<D: Digest>` hache avec SHA-256, SHA-384 ou SHA-512 sans écrire trois fois la
     boucle (`crates/mega-core/src/digital_signature/cms.rs:92`).
   - Fiche : [traits](concepts/traits-et-objets-de-trait.md).
8. **Matrices affines et inversion.**
   - Pour savoir quels pixels d'une image tombent sous la zone, on inverse la matrice qui pose
     l'image sur la page (`crates/mega-core/src/content/raster.rs:74`), puis on projette les
     coins de la zone (`raster.rs:122`).
   - La matrice courante `cm` se compose en lisant le flux (`raster.rs:57`, `raster.rs:425`).
   - Fiches : [matrices](concepts/matrices-et-transformations.md),
     [flux de contenu](concepts/flux-de-contenu-pdf.md).
9. **`BTreeMap` comme table d'attribution.**
   - Chaque caractère reçoit un identifiant (CID) à sa première apparition ;
     `cids.get(&c)` retrouve les suivants (`crates/mega-core/src/fonts/unicode.rs:154`).
   - Fiche : [complexité et collections](concepts/complexite-et-collections.md).
10. **Côté interface : un snippet Svelte avec argument.**
    - `MenuButton` reçoit ses éléments sous forme d'un snippet qui prend `pick` en paramètre
      (`app/src/lib/components/MenuButton.svelte:19`).
    - `inGroup` ferme un groupe dans un `finally`, même après une erreur
      (`app/src/lib/logic/group.ts:8`).

## Décisions de conception (ADR-0014)

- **Helvetica reste le défaut** : la police Unicode ne sert que hors WinAnsi, et les fichiers
  d'avant ne changent pas.
- **Deux chemins d'écriture du texte Unicode** :
  - `lopdf` écrit une police `Type0` dont il ne garde que les glyphes utilisés (sous-ensemble
    par `subsetter`, quelques Ko) ;
  - PDFium exige une police complète : `cleanup::dedupe_font_files`
    (`crates/mega-core/src/cleanup.rs:96`) n'en garde qu'un exemplaire par fichier.
- **Caviardage des images par `lopdf`, pas par PDFium** : pdfium-render ne lit ni le masque de
  transparence ni l'espace de couleur. Réécrire l'image par PDFium aurait rendu opaques les
  parties transparentes d'un logo.
- **Toujours une nouvelle image** : l'image d'origine peut servir sur d'autres pages, qui la
  gardent. Si plus rien ne la cite, l'élagage la retire du fichier.
- **Chiffrement : on rechiffre avec l'état d'origine** (même clé, même `/ID`). Le mot de passe
  et les permissions ne changent pas.
- **Tampon de signature placé par coin dans l'interface** : le CLI accepte un rectangle libre.
  Le tampon n'est pas la preuve, la signature cryptographique l'est.

## Pièges rencontrés

- **Les anciennes images restaient dans le fichier** :
  - symptôme : le test « motif secret absent du fichier enregistré » échouait ;
  - cause : le nom de l'image remplacée restait dans `/Resources /XObject`, donc l'image
    restait atteignable et l'élagage ne la retirait pas ;
  - correction : ôter les noms devenus inutiles (`crates/mega-core/src/content/raster.rs:515`).
- **Une police chargée par PDFium appartient à un `PdfDocument`** : après un rechargement,
  l'ancien jeton ne vaut plus rien. Le cache est remis à zéro dans `replace_inner`
  (`crates/mega-core/src/document.rs:212`).
- **Signature sur une page tournée** :
  - le rectangle est en espace affiché, PDFium doit le convertir ;
  - PDFium et `lopdf` ne peuvent pas travailler en même temps (verrou non réentrant), donc la
    conversion est faite avant (`crates/mega-core/src/digital_signature/pdf.rs:839`) ;
  - la rotation va dans la `/Matrix` de l'apparence (`pdf.rs:867`).
- **Git Bash réécrit les arguments qui commencent par `/`** : `/// doc` devenait `//// doc`,
  et clippy l'a signalé (`four_forward_slashes`). Solution : `MSYS_NO_PATHCONV=1`. Une
  apostrophe dans une chaîne `'…'` du shell coupe aussi la chaîne (`b'x'` devenu `bx`).
- **Scénario CDP non rejouable** : le mode « page unique » est enregistré dans les
  préférences. Au second passage, le bouton portait l'autre libellé. L'étape clique désormais
  seulement si le bouton est là.
- **Un attribut CAdES en double était ignoré** : la recherche d'attribut rend `None` en cas
  de doublon, donc la vérification du certificat était sautée. Le doublon est maintenant
  refusé.

## Exercices de lecture

1. `crates/mega-core/src/fonts/unicode.rs:154` à `:182` : pourquoi faut-il encoder **tous**
   les textes d'une passe avant d'appeler `write` ? Que se passerait-il si on écrivait la
   police après le premier texte seulement ?
2. `crates/mega-core/src/content/raster.rs:122` : la zone est élargie d'un pixel et arrondie
   « vers l'extérieur ». Dans quel sens se tromper est-il acceptable pour un caviardage, et
   pourquoi ?
3. `crates/mega-core/src/edit.rs:606` : une édition dans un groupe déjà commencé ne pousse
   pas d'instantané. Pourquoi l'annulation ramène-t-elle quand même l'état d'avant la
   **première** page ?

## Prérequis pour la suite

- Lire un flux de contenu PDF et suivre la matrice courante (fiches *matrices* et *flux de
  contenu*).
- Distinguer emprunt et consommation (`&self`, `&mut self`, `self`).
- Comprendre ce qu'une signature garantit (intégrité) et ce qu'elle ne garantit pas
  (identité certifiée) : voir `docs/apprentissage/lot-8.md`.
