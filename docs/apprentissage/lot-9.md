# Lot 9 — Consolidation : note d'apprentissage

## Ce que le lot a construit

Aucune grande fonctionnalité nouvelle : le lot reprend les constats « mineurs » laissés par
les relectures des Lots 0 à 8 (110 points triés, environ 80 corrigés). Côté utilisateur : un
caviardage qui ne laisse plus fuir le texte d'une note par son popup, un CLI qui n'écrase plus
un fichier sans `--force`, un zoom qui reste sous le curseur, des menus et des onglets
utilisables au clavier, des signatures mieux lues et mieux écrites, et trois petits ajouts
(redimensionner une image, chemin de Tesseract dans les préférences, ouverture depuis le
Finder sous macOS). Version 0.9.0.

## Concepts Rust rencontrés

### 1. Une table constante (`crates/mega-core/src/fonts.rs:24`)

`const HELVETICA_WIDTHS: [u16; 224]` : les largeurs d'Helvetica sont **dans le binaire**. Le
compilateur vérifie qu'il y a exactement 224 valeurs ; `helvetica_width` (ligne 54) lit la
table avec `get`, qui rend `None` hors limites au lieu de paniquer.

### 2. `#[cfg(…)]` contre `cfg!(…)` (`crates/mega-core/src/engine.rs:81`)

`cfg!(debug_assertions)` est une **valeur** (`true`/`false`) : les deux branches sont compilées.
`#[cfg(debug_assertions)]` est un **attribut** : la fonction n'existe pas du tout dans l'autre
profil. Ici, le chemin de la machine de compilation ne peut donc plus finir dans un binaire
distribué.
- Fiche : [`profils-cargo-et-distribution`](concepts/profils-cargo-et-distribution.md).

### 3. Lire sans dépasser une limite : `Read::take` (`crates/mega-core/src/attachments.rs:119`)

Vérifier la taille d'un fichier **puis** le lire laisse une fenêtre où il peut grossir.
`File::open(path)?.take(MAX + 1).read_to_end(&mut v)` lit au plus `MAX + 1` octets : si on en
a lu plus que `MAX`, le fichier est trop gros, et il n'a jamais été lu en entier.
- Fiche : [`read-write-et-cursor`](concepts/read-write-et-cursor.md).

### 4. Égalité et ordre cohérents (`crates/mega-core/src/service/queue.rs:39`)

`BinaryHeap` exige `Ord`, donc `Eq`. L'ancien code comparait l'élément transporté avec une
égalité **toujours fausse** : `a == a` était faux, alors que `a.cmp(&a)` disait « égal ».
Désormais `PartialEq` compare (priorité, séquence), comme `Ord` : les deux disent la même chose,
et l'élément n'a plus besoin d'être comparable du tout (`impl<T>` sans `T: Eq`).
- Fiche : [`traits-et-objets-de-trait`](concepts/traits-et-objets-de-trait.md).

### 5. Choisir sa collection : `BTreeMap` / `BTreeSet` (`crates/mega-core/src/service/cache.rs:37`, `crates/mega-core/src/cleanup.rs:64`)

« Déjà vu ? » sur un `Vec` coûte un parcours (`contains`, temps proportionnel à la taille) ;
sur un `BTreeSet`, un logarithme. Le cache des pages range ses dates d'utilisation dans un
`BTreeMap` (le premier élément est le moins récent) ; l'élagage des objets PDF suit le graphe
avec un `BTreeSet` au lieu du `Vec` de `lopdf`.
- Fiche : [`complexite-et-collections`](concepts/complexite-et-collections.md).

### 6. Fonction nommée contre fermeture : les durées de vie (`crates/mega-core/src/properties.rs:145`)

La fermeture `|v: &Option<String>| v.as_deref().map(str::trim)` ne compile pas : le compilateur
ne sait pas que le `&str` rendu vit aussi longtemps que l'`Option` prêtée. Une `fn trimmed(v:
&Option<String>) -> Option<&str>` le dit par **élision** : une seule référence en entrée, donc
la sortie emprunte celle-là.
- Fiche : [`ownership-et-emprunts`](concepts/ownership-et-emprunts.md).

### 7. Un emprunt rendu par un bloc (`crates/mega-core/src/content/mod.rs:392`)

`t.font()` emprunte l'objet texte `t` ; `t.set_text(…)` veut l'emprunter **en écriture**. On
calcule ce qu'il faut savoir de la police dans un bloc `{ … }` : à la fin du bloc l'emprunt
partagé est rendu, et l'emprunt exclusif devient possible.

### 8. Parcourir un graphe : atteignabilité (`crates/mega-core/src/cleanup.rs:251`)

Supprimer une note de `/Annots` ne la retire pas du fichier tant qu'un autre objet la cite
(son popup, une réponse, l'arbre de structure). L'élagage garde tout ce qui est **atteignable**
depuis la remorque ; il faut donc d'abord couper ces liens (`drop_dangling_annotation_links`,
ligne 253).
- Fiche : [`flux-de-contenu-pdf`](concepts/flux-de-contenu-pdf.md) (structure d'un PDF).

## Décisions de conception

- Les constats qui demandent une vraie nouvelle fonctionnalité (police Unicode incorporée,
  caviardage raster, nettoyage d'un document chiffré) sont **hors périmètre**, mais là où le
  texte serait abîmé, un refus explicite remplace le silence (spec du lot, ruling 2).
- Les verdicts de signature portent un **code stable** (`SigIssue`,
  `crates/mega-core/src/digital_signature/mod.rs:34`) que l'interface traduit ; le CLI affiche
  `SigIssue::message()`. Avant, l'interface anglaise montrait des raisons en français.
- Les largeurs Helvetica (AFM d'Adobe) servent à la fois aux `/Widths` de nos polices et à
  l'étirement des mots OCR : un seul calcul, le même pour tous les lecteurs.
- `/NeedAppearances` n'est retiré à la signature que si **tous** les champs ont déjà une
  apparence : sinon ils s'afficheraient vides ailleurs.

## Pièges rencontrés

- **Un test qui passe avant la correction.** Plusieurs constats n'avaient pas d'effet
  observable (orphelins du CLI, compression d'une image brute, page tournée en OCR) : le
  test passait d'emblée. Ils sont restés comme garde-fous, et la décision est consignée ; B30
  a trouvé son test plus tard (la date `/ModDate` posée par le nettoyage).
- **Une sonde vaut mieux qu'une hypothèse.** Avant de refuser du texte hors WinAnsi dans un
  champ, un rendu a montré que PDFium dessine « Ω » avec une police de repli : le refus aurait
  cassé une saisie qui marchait. De même, aucune corruption testée ne faisait échouer `lopdf`
  tout en laissant PDFium ouvrir le fichier : le refus prévu pour ce cas était inutile.
- **Les fixtures de test sont du code.** Nos formes sont des `Ink` (pas des `Square`), une
  annotation fraîche est écrite en ligne dans `/Annots`, une page de test sans `/Resources`
  fait râler `qpdf`, et `sample_pdf` changeait d'octets à chaque génération (`/ID` aléatoire).
- **L'ordre des vérifications compte.** Un commit est parti pendant que clippy échouait (deux
  commandes enchaînées) : corrigé par un commit séparé. Clippy se lance seul, avant.
- **Les scénarios CDP mesurent le DOM après coup.** L'indicateur de dépôt n'existe qu'après la
  mise à jour de Svelte ; la mesure se fait donc dans une étape suivante.

## Exercices de lecture

1. `crates/mega-core/src/service/mod.rs:749` (`step`) : pourquoi la boucle aspire-t-elle
   **toutes** les requêtes arrivées avant d'en traiter une ? Que se passerait-il sinon pour une
   page visible demandée après dix vignettes ?
2. `crates/mega-core/src/digital_signature/cms.rs:202` (`signed_attrs_span`) : pourquoi
   remplacer l'étiquette `0xA0` par `0x31` avant de vérifier la signature ?
3. `app/src/lib/logic/queue.ts:9` (`serialQueue`) : pourquoi `tail` garde-t-il `run.catch(…)`
   et non `run` lui-même ? Que deviendrait la file après une tâche en échec ?

## Prérequis pour la suite

Aucun lot n'est planifié après celui-ci. Pour reprendre le code à son rythme : ownership et
emprunts, `Result` et `?`, les traits (`Ord`, `PartialEq`), les collections de la bibliothèque
standard et leur coût, et le patron acteur (`canaux-et-acteur`).
