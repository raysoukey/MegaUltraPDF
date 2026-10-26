# Lot 2 — Pages et documents : note d'apprentissage

## Ce que le lot a construit

Les premières **modifications** de documents : pivoter, supprimer, réordonner (glisser-déposer
des vignettes), extraire, insérer (page vide ou pages d'un autre PDF), fusionner, scinder
(par plages, toutes les N pages, par signets), enregistrer et enregistrer sous, avec
annuler/rétablir, indicateur « modifié » sur l'onglet et confirmation à la fermeture. Le CLI
gagne `merge`, `split`, `rotate` et `extract`. Côté Rust, les nouveautés sont le module
`edit` (patron Command + historique par instantanés), le module `pages` (opérations PDFium) et
`PageSpec` (analyse des plages « 1-3,5,8- »). Décisions : ADR-0007.

## Concepts Rust rencontrés

### 1. Traits et objets de trait : `Command` et `&dyn Command`

`edit.rs` : un trait à deux méthodes (`label`, `apply`) et un historique qui ne connaît que
`&dyn Command`. L'interface envoie une valeur `EditCommand` (enum sérialisée), le backend
l'applique. Ajouter une variante fait échouer le `match` exhaustif : le compilateur tient la
liste.

- Fiche : [`traits-et-objets-de-trait`](concepts/traits-et-objets-de-trait.md).

### 2. Memento : annuler en rechargeant un instantané

Plutôt que d'écrire l'inverse de chaque édition, `Document::apply` sérialise le document
avant d'agir (`to_bytes`) et empile les octets ; `undo` recharge. `VecDeque` pour la pile
d'annulation (on retire par l'avant quand on dépasse `HistoryLimits`), `Vec` pour la pile de
rétablissement. `is_modified` compare deux **positions** dans la ligne du temps, pas deux
fichiers : relire la doc de `History` et le test `undo_back_to_saved_state_is_not_modified`.

### 3. `Option<u64>` comme machine à états

`saved_position: Option<u64>` : `Some(n)` = « l'état enregistré est à la position n »,
`None` = « l'état enregistré n'est plus atteignable » (une édition après un annuler a coupé la
branche). Comparer `Option` avec `>` fonctionne (`None < Some(_)`), mais relisez
`Document::apply` pour voir pourquoi on teste `saved_position > Some(position)`.

### 4. Écriture atomique : `tempfile` + `persist`

`edit::write_atomically` : fichier temporaire **dans le même dossier** que la cible (un
renommage n'est atomique qu'à l'intérieur d'un même système de fichiers), `sync_all` pour
forcer l'écriture sur le disque, puis `persist` qui renomme par-dessus. Le test CLI
`extract_pages_and_reject_bad_spec` vérifie qu'un échec laisse l'ancien fichier intact.

### 5. `BTreeSet` pour trier et dédoublonner

`PageSpec::parse` et `split_at` collectent dans un `BTreeSet<u16>` : trié, sans doublon,
en une ligne. Comparez avec un `Vec` suivi de `sort` + `dedup` : même résultat, plus de code.

### 6. `clap` : `conflicts_with`, `required`, sous-commandes riches

`crates/mega-cli/src/cli.rs` : `--ranges`, `--every` et `--bookmarks` s'excluent
(`conflicts_with_all`) ; `merge` exige au moins un fichier (`#[arg(required = true)]` sur un
`Vec`). `clap` génère les messages d'erreur et l'aide à partir des commentaires `///`.

### 7. Côté TypeScript (pour mémoire)

Les règles métier restent en Rust : la prévisualisation d'une scission appelle la commande
`split_plan` plutôt que de réanalyser « 1,2-3 » en JS. Le frontend garde seulement la
logique d'**interface** : sélection multiple (`clickSelection`) et calcul d'un dépôt de
glisser-déposer (`reorder`, `moveTarget`), toutes deux pures et testées en Vitest.

## Décisions de conception du lot

ADR-0007 (Command, memento, écriture atomique). Décisions consignées pendant l'exécution :
`extract_pages`/`insert_pages_from` respectent l'ordre demandé ; `move_pages` prend l'index
d'insertion **après retrait** des pages déplacées ; `Save` ne vide pas le cache de rendu ;
un document fusionné s'ouvre sans chemin, donc « modifié » d'emblée.

## Pièges rencontrés

- **Taille d'une page pivotée** : PDFium rapporte largeur et hauteur *après* `/Rotate` ; les
  vignettes et la géométrie l'utilisent tel quel, il ne faut pas réappliquer la rotation.
- **`pdfium-render` n'a pas `FPDF_MovePages`** : déplacer = copier depuis un instantané puis
  supprimer ; les signets qui visaient ces pages peuvent être perdus (documenté).
- **Documents chiffrés** : les instantanés sont rechargés sans mot de passe (PDFium
  sauvegarde déchiffré) ; à revoir avec le chiffrement du Lot 6.
- **Glisser-déposer HTML5 sous WebView2** : le scénario CDP simule des `DragEvent` avec un
  `DataTransfer` maison plutôt que de vrais mouvements de souris (le glisser natif bloque la
  boucle d'événements de l'automatisation).
- **Chaînes de caractères et outils** : un script Python ou un heredoc qui contient des
  antislashs doit être écrit dans un fichier, pas tapé dans la ligne de commande (un test de
  chemin Windows a perdu ses `\\`).

## Vérification sans souris

`scripts/ui-check.ps1 -Pdf <copie de simple.pdf> -Scenario scripts/scenarios/edition-pages.json`
déroule la checklist de la spec : rotation de deux pages, déplacement par glisser-déposer,
suppression, annuler/rétablir, insertion, enregistrement (la marque « • » disparaît),
dialogues Fusionner et Scinder (aperçu, plage invalide), confirmation à la fermeture.
Après le scénario, la copie a 3 pages dans l'ordre 3, 1, 2 (les pages 1-2 pivotées) :
`mega extract copie.pdf --pages 2 -o p.pdf` puis `mega info p.pdf` montre `Rotation page 1 : 90°`.

## Exercices de lecture

1. `edit.rs`, `History::push_undo` : pourquoi garder **toujours** l'instantané le plus récent
   même s'il dépasse `max_bytes` ? Que verrait l'utilisateur sinon ?
2. `pages.rs`, `move_pages` : suivez les index. Si l'on déplace les pages `[1, 3]` d'un
   document de 5 pages vers `to = 0`, quelles pages sont copiées, dans quel ordre, et
   lesquelles sont ensuite supprimées ?
3. `app/src/lib/logic/pageselection.ts`, `moveTarget` : pourquoi passer par `reorder` pour
   détecter un dépôt sans effet, plutôt que de comparer `slot` aux pages sélectionnées ?

## Prérequis pour le Lot 3

Les annotations : lire le modèle d'annotations de PDF (`/Annots`, apparences `/AP`),
comprendre ce que `pdfium-render` expose (création, propriétés, rendu) et comment un calque
SVG au-dessus de la page suit le zoom et la rotation (géométrie du Lot 1).
