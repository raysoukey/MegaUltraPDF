# Lot 11 — Lecteur PDF par défaut : ce que vous venez de voir

## Ce que le lot a construit

- **Une seule fenêtre** : un PDF ouvert depuis l'Explorateur ou le gestionnaire de fichiers
  pendant que MegaUltraPDF tourne arrive dans un **nouvel onglet** de la fenêtre existante.
  Plusieurs fichiers sélectionnés d'un coup s'ouvrent tous.
- **Préférences ▸ Lecteur PDF par défaut** : l'application dit si elle ouvre les PDF du système
  et aide à le devenir, selon ce que chaque système permet.
- L'installateur Windows enregistre l'application auprès des « Applications par défaut ».
- Version 0.11.0.

## Concepts Rust rencontrés

1. **Fermeture passée à un greffon.** `tauri_plugin_single_instance::init(|app, argv, cwd| …)`
   reçoit une fermeture appelée à chaque seconde instance (`app/src-tauri/src/lib.rs`, dans
   `run()`). Fiche : [itérateurs et fermetures](concepts/iterateurs-et-fermetures.md).
2. **`std::path::absolute` contre `canonicalize`.**
   - `absolute` rend un chemin absolu sans toucher au disque ; on le garde pour l'affichage.
   - `canonicalize` suit les liens et ajoute `\\?\` sous Windows ; il sert seulement à repérer
     deux écritures du même fichier (`commands.rs`, `pdf_paths_from_args`).
3. **`BTreeSet` pour éliminer les doublons** en gardant l'ordre d'arrivée dans un `Vec` à part
   (même fonction). Fiche : [complexité et collections](concepts/complexite-et-collections.md).
4. **`cfg!` contre `#[cfg]`.**
   - `cfg!(windows)` est un booléen connu à la compilation : les deux branches doivent compiler
     partout, ce qui garde les fonctions d'analyse testables sur toutes les machines de la CI
     (`default_app.rs`).
   - `#[cfg(windows)]` retire le code des autres systèmes : c'est ce qu'il faut pour
     `CommandExt::creation_flags`, qui n'existe que sous Windows.
5. **Lancer un programme sans shell** : `std::process::Command::new("reg").args([...])`. Les
   arguments ne sont jamais réinterprétés par un interpréteur de commandes, ce qui est plus sûr.
6. **Analyser une sortie texte dans une fonction pure.** `parse_reg_progid` et
   `parse_xdg_default` se testent sur des sorties réelles copiées dans les tests, sans lancer
   les programmes.
7. **Attendre avec une limite** : `try_wait()` demande « fini ? » sans bloquer, ce qui permet
   d'arrêter (`kill`) un programme qui ne répond pas (`default_app.rs`, fonction `run`).
8. **Commande Tauri `async` et `spawn_blocking`** : un travail qui attend un autre programme
   part sur un fil réservé aux tâches bloquantes ; la fenêtre reste fluide (`commands.rs`,
   `default_app_status`).
9. **Dépendance réservée à certains systèmes** : `[target.'cfg(...)'.dependencies]` dans
   `app/src-tauri/Cargo.toml`, et le même `#[cfg(...)]` devant son utilisation (`lib.rs`).

## Décisions de conception (ADR-0015)

- **Windows interdit à une application de se désigner elle-même** comme lecteur par défaut :
  le bouton ouvre la fiche de MegaUltraPDF dans les Paramètres, et c'est l'utilisateur qui
  choisit.
- **Linux le permet** (`xdg-mime default`). **macOS** demande le Finder : l'application affiche
  la marche à suivre.
- **Fiche dans les Paramètres** : elle n'existe que si l'application est déclarée dans
  `RegisteredApplications`. C'est le greffon NSIS `windows/hooks.nsh` qui s'en charge, et il la
  retire à la désinstallation.

## Pièges rencontrés

- **Un scénario CDP comptait trop d'onglets** : le sélecteur `.tabs [role=tab]` comptait aussi
  les onglets de la barre latérale (Vignettes, Signets…). Le bon sélecteur est la liste des
  documents, `[role=tablist][aria-label=MegaUltraPDF]`.
- **L'application rouvre les documents mémorisés** : un scénario ne peut pas supposer un seul
  onglet au départ. Il mesure l'écart avant et après.
- Le **ProgId** de l'association Windows est le nom donné dans `tauri.conf.json`
  (`MegaUltraPDF.Document`), vérifié dans le registre après une vraie installation.

- **Relecture** : une installation MSI passait pour une version portable (on cherchait
  `uninstall.exe`, propre à NSIS). On regarde maintenant la commande d'ouverture du ProgId,
  écrite par les deux installateurs. Windows 11 range aussi son choix dans
  `UserChoiceLatest`, lu en premier.
- **Relecture** : deux ouvertures simultanées du même fichier pouvaient créer deux onglets ;
  elles passent maintenant l'une après l'autre (`openQueue`, `documents.svelte.ts`).

## Exercices de lecture

1. `app/src-tauri/src/lib.rs`, `second_instance_paths` : pourquoi saute-t-on le premier
   argument, et pourquoi lit-on les chemins relatifs dans le dossier de la **seconde**
   instance ?
2. `app/src-tauri/src/default_app.rs`, `classify` : quelles sont les deux façons dont Windows
   peut désigner MegaUltraPDF comme lecteur par défaut ?
3. `app/src-tauri/windows/hooks.nsh` : que se passerait-il, côté Paramètres de Windows, si on
   oubliait la clé `RegisteredApplications` ?

## Prérequis pour la suite

- Distinguer la logique du document (`mega-core`) de l'intégration au système
  (`app/src-tauri`).
- Savoir lire un message d'erreur de processus externe (code de sortie, sortie standard).
