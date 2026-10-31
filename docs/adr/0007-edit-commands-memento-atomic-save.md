# ADR-0007 — Éditions de pages : patron Command, historique par instantanés, écriture atomique

Date : 2026-10-26 · Statut : accepté (Lot 2)

## Contexte

Le Lot 2 introduit les premières **modifications** de documents : pivoter, supprimer,
déplacer, insérer des pages, puis enregistrer. Trois exigences de la spec guident la
conception : toute édition doit être annulable et rétablissable, un enregistrement ne doit
jamais laisser un fichier corrompu (même en cas de coupure de courant au milieu), et la
logique doit être partagée entre l'application et la ligne de commande. PDFium
(`pdfium-render` 0.9) sait réécrire un document (`save_to_bytes`) mais ne propose aucun
historique ni aucune écriture sûre.

## Décision

- **Une édition = une valeur de données.** `mega_core::EditCommand` est une `enum` sérialisable
  (`serde`, étiquette `kind` en `snake_case`), miroir exact du type TypeScript `EditCommand`
  dans `app/src/lib/api.ts`. Le frontend décrit **ce qu'il veut**, jamais comment ; le
  backend applique. Le trait `Command { label, apply }` isole le « comment » : `EditCommand`
  l'implémente, et un futur type (annotation, formulaire) pourra l'implémenter aussi sans
  toucher à l'historique, qui ne manipule que `&dyn Command`.
- **Historique par instantanés (memento), pas par commandes inverses.** Avant chaque
  `apply`, le document complet est sérialisé (`save_to_bytes`) et empilé ; annuler recharge
  l'instantané. Les inverses exacts seraient fragiles (une suppression de page perd des
  signets et des liens que l'on ne saurait pas recréer), alors qu'un instantané est toujours
  juste. Le coût mémoire est borné par `HistoryLimits` (20 étapes, 256 Mo ; le plus récent
  est toujours conservé).
- **« Modifié » = position dans la ligne du temps ≠ position enregistrée.** `History` compte
  les éditions appliquées (`position`) et retient la position au dernier enregistrement
  (`saved_position`). Annuler jusqu'à l'état enregistré rend `is_modified` faux sans
  comparer d'octets ; une nouvelle édition après un annuler rend l'état enregistré
  inatteignable (`saved_position = None`).
- **Écriture atomique partout.** `edit::write_atomically(path, bytes)` écrit dans un fichier
  temporaire **du même dossier** (`tempfile::Builder::tempfile_in`), force l'écriture sur
  disque (`sync_all`), puis renomme par-dessus la cible (`persist`). Enregistrer, Enregistrer
  sous, extraire, fusionner (CLI) et scinder passent tous par là. Un échec laisse l'ancien
  fichier intact ; vérifié par `save_as_to_directory_path_leaves_original_intact_and_no_temp_file`
  (échec du renommage après écriture du temporaire) et `save_as_failure_leaves_original_intact`.
- **L'acteur reste le seul à toucher aux documents.** Les requêtes `Apply`, `Undo`, `Redo`,
  `Save`, `ExtractPages`, `MergeFiles`, `SplitDocument` s'ajoutent à l'`enum Request`
  (ADR-0006). Une édition invalide le cache de rendu du document ; un enregistrement ne
  l'invalide pas (les pages ne changent pas).
- **L'analyse des plages de pages vit en Rust** (`PageSpec`), utilisée par le CLI et exposée
  à l'interface par la commande `split_plan` (prévisualisation d'une scission). Aucune règle
  métier n'est dupliquée en TypeScript ; le frontend ne contient que la sélection de vignettes
  et le calcul d'un dépôt de glisser-déposer, qui sont de la logique d'interface (testée).

## Alternatives écartées

- **Commandes inverses** (`RotatePages(-n)`, réinsertion des pages supprimées) : plus
  économes, mais incomplètes face aux structures annexes d'un PDF (signets, liens, formulaire).
- **Historique côté frontend** : aurait obligé à dupliquer la sémantique des éditions en
  TypeScript et à faire transiter des documents entiers par IPC.
- **Écriture directe suivie d'une copie de sauvegarde `.bak`** : laisse un fichier à moitié
  écrit pendant l'opération et des fichiers orphelins ; le renommage atomique est la
  pratique standard.

## Conséquences

- Chaque `apply` sérialise le document : sur de très gros fichiers, une édition prend le
  temps d'un enregistrement. À mesurer ; une stratégie d'instantanés différés pourra venir
  plus tard sans changer l'API.
- PDFium réenregistre un document chiffré **chiffré** (même mot de passe) : `Document`
  conserve le mot de passe d'ouverture et le réutilise pour recharger ses instantanés
  (annuler, rétablir, déplacer). Si un rechargement échoue, l'étape reste dans l'historique.
  (Révision après relecture : la première version supposait l'inverse et perdait l'étape.)
- Si une commande échoue après avoir commencé à modifier le document (plusieurs appels
  PDFium), `Document::apply` recharge l'instantané pris juste avant : le contrat « inchangé
  en cas d'erreur » tient pour toute commande, pas seulement pour les échecs de validation.
- Un document issu d'une fusion n'existe pas sur disque : `mark_never_saved` le rend
  « modifié » jusqu'à « Enregistrer sous », quoi qu'on annule.
- `write_atomically` conserve les permissions du fichier remplacé (POSIX) et suit les liens
  symboliques. Depuis le Lot 13 (audit P4, P6) :
  - un fichier en lecture seule est refusé (`CoreError::FileReadOnly`) ;
  - un fichier verrouillé par une autre application donne `FileLocked` ;
  - un dossier interdit en écriture donne `FolderNotWritable`.

  Chaque message propose « Enregistrer sous ». Le dossier est rendu canonique (`\\?\` sous
  Windows), ce qui permet d'enregistrer au-delà de 260 caractères. Sous Unix, un fichier à
  plusieurs liens physiques est écrit **en place** après une copie de sécurité synchronisée,
  pour que les autres noms voient la nouvelle version.

  **Exception assumée** : sous Windows, les ACL explicites et les flux de données alternatifs
  (ADS) du fichier remplacé sont perdus au renommage. Les garder demanderait `ReplaceFileW`,
  donc du code `unsafe`, interdit dans ce projet.
- `split_to_dir` refuse d'écraser un fichier existant (sauf `--force` au CLI) et un préfixe
  qui sort du dossier ; `split_to_dir` refuse d'écraser un fichier existant (sauf `--force` au CLI)
  et un préfixe qui sort du dossier.
- `move_pages` est implémenté par copie depuis un instantané puis suppression
  (`pdfium-render` n'expose pas `FPDF_MovePages`) : les signets qui visaient les pages
  déplacées peuvent être perdus. Documenté, à revoir avec `lopdf` (ADR-0003).
- Tests : `crates/mega-core/tests/{pages,edit,service}.rs`, `crates/mega-cli/tests/cli.rs`,
  scénario CDP `scripts/scenarios/edition-pages.json`.
