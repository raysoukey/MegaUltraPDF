# Lot 11 — MegaUltraPDF comme lecteur PDF par défaut

## Contexte

L'utilisateur a demandé si le logiciel peut être défini comme lecteur PDF par défaut du
système. Aujourd'hui seule une partie existe : les installateurs déclarent l'association
`.pdf` (`fileAssociations` de `app/src-tauri/tauri.conf.json`). L'application apparaît donc
dans « Ouvrir avec » et dans les applications par défaut, mais trois manques gênent l'usage
quotidien :

1. **Plusieurs instances** : double-cliquer un PDF quand l'application est déjà ouverte lance
   une seconde fenêtre (pas de greffon single-instance). Ouvrir plusieurs fichiers depuis
   l'Explorateur ouvre autant de fenêtres. `startup_file` ne lit d'ailleurs qu'**un**
   argument (`commands.rs:529-546`).
2. **Aucun moyen depuis l'application** de savoir si MegaUltraPDF est le lecteur par défaut,
   ni de le devenir.
3. **Rôle « Viewer »** déclaré, alors que l'application édite aussi les PDF (rôle
   « Editor » sur macOS).

Résultat visé, sur chaque système : un clic sur un PDF ouvre un **onglet** dans la fenêtre
existante. Les préférences indiquent si MegaUltraPDF est le lecteur par défaut et proposent
de le devenir, avec la méthode que chaque système permet. Release **v0.11.0** à la fin, comme
pour le Lot 10.

## Processus (inchangé)

- `/lot 11`, puis la spec `docs/superpowers/specs/2026-10-28-lot-11-lecteur-par-defaut.md`
  et le plan `docs/superpowers/plans/…`.
- Branche `lot-11/lecteur-par-defaut`, exécution native avec le registre.
- TDD (RED vu, puis GREEN).
- Vérifications CDP et manuelles, `/verif` 8/8.
- Relecture Opus, puis une seule passe de correction test d'abord.
- `/note-apprentissage`, PR, CI sur 3 OS, fusion, release v0.11.0.

## A. Une seule instance, fichiers ouverts en onglets

- Nouvelle dépendance **`tauri-plugin-single-instance` 2** (greffon officiel Tauri),
  justifiée dans le commit.
  - Enregistrée **en premier** dans `run()` (`app/src-tauri/src/lib.rs`), comme le demande la
    documentation du greffon.
  - Son rappel `(app, argv, cwd)` :
    - extrait les chemins PDF ;
    - les pousse dans `PendingPaths` (`lib.rs:55`) ;
    - émet `open-paths` ;
    - ramène la fenêtre au premier plan (`unminimize`, `set_focus`).
  - L'interface écoute déjà `open-paths` et vide `take_pending_paths`
    (`+page.svelte:147-150`) : rien à changer côté Svelte.
- **Fonction pure, testée** : `pdf_paths_from_args(args, cwd) -> Vec<String>` dans
  `commands.rs`.
  - Elle remplace la logique de `startup_file_from_args` : **tous** les PDF existants, et non
    le premier seulement.
  - Un chemin relatif est résolu par rapport au `cwd` de la seconde instance.
  - Les options (`-…`) et les non-PDF sont ignorés, ainsi que les doublons.
- **Démarrage à froid** : `startup_file` devient `startup_files() -> Vec<String>` (commande
  Tauri, avec `api.ts` et l'appel dans `+page.svelte`, qui ouvre tous les fichiers).
- macOS reste sur `RunEvent::Opened`, déjà en place (une application macOS est une seule
  instance par nature).
- Tests Rust :
  - `pdf_paths_from_args_keeps_every_existing_pdf` ;
  - `relative_paths_resolve_against_the_second_instance_dir` ;
  - `options_and_non_pdf_are_ignored` ;
  - `duplicates_are_dropped`.

## B. Lecteur par défaut : état et bouton dans les préférences

Nouveau module `app/src-tauri/src/default_app.rs`. C'est une intégration au système, sans
logique PDF : elle a sa place dans `mega-app`, pas dans `mega-core`. Commandes lancées sans
shell (liste d'arguments), comme Tesseract.

**Commande `default_app_status() -> DefaultAppStatus`.** Union sérialisée
`{ kind: 'default' | 'other' | 'unknown', handler: string | null, can_set: bool }`.
- **Windows** :
  - lecture de
    `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.pdf\UserChoice\ProgId`
    par `reg query`, sans nouvelle dépendance ;
  - le résultat est comparé au ProgId de l'installateur (`MegaUltraPDF.Document`, nom de
    l'association) ;
  - analyse de la sortie dans une fonction pure, testée sur des sorties réelles en fixture
    texte.
- **Linux** : `xdg-mime query default application/pdf`, comparé au nom du fichier `.desktop`
  du paquet. Le nom se vérifie dans le `.deb` produit ; il est consigné dans une constante.
- **macOS** : `unknown`. Lire le gestionnaire exigerait une API système (FFI) ; une
  dépendance de plus n'est pas justifiée pour ce lot.

**Commande `make_default_app() -> DefaultAppAction`.** Ce qui a été fait, pour le message.
- **Windows** : Windows 10/11 interdit à une application de se désigner elle-même (empreinte
  `UserChoice` protégée). On ouvre donc les **paramètres** au bon endroit, via
  `tauri-plugin-opener`, déjà présent :
  - `ms-settings:defaultapps?registeredAppUser=MegaUltraPDF` si l'application est enregistrée
    (voir C) ;
  - sinon `ms-settings:defaultapps`.
  - Action `opened_settings`.
- **Linux** : `xdg-mime default <desktop> application/pdf`, puis relecture de l'état. Action
  `set`. Une AppImage, ou l'absence de `xdg-mime`, donne `unsupported` avec une explication.
- **macOS** : action `instructions` : l'interface affiche la marche à suivre (Lire les
  informations, Ouvrir avec, Tout modifier).
- Version **portable** (`.zip`), détectée par l'absence de désinstalleur à côté de
  l'exécutable : `unsupported`, avec le message « installez l'application pour l'associer aux
  PDF ».

**Interface (`PreferencesDialog.svelte`)** : section « Lecteur PDF par défaut » avec l'état
(« MegaUltraPDF est le lecteur par défaut », « Un autre programme ouvre les PDF », « État
inconnu ») et un bouton « Définir comme lecteur par défaut ».
- Après l'action :
  - Linux relit l'état ;
  - Windows affiche « Choisissez MegaUltraPDF dans la fenêtre des paramètres » ;
  - macOS affiche la marche à suivre.
- Chaînes `strings.ts` en fr **et** en.
- Logique pure `logic/defaultapp.ts` (texte à afficher selon l'état et l'action), avec tests
  Vitest.

## C. Déclarations des paquets

- `tauri.conf.json` : rôle de l'association `Viewer` → `Editor`.
- **Windows NSIS** : greffon d'installation (`bundle.windows.nsis.installerHooks`, fichier
  `app/src-tauri/windows/hooks.nsh`).
  - À l'installation, il écrit les **Capabilities** :
    - `Software\MegaUltraPDF\Capabilities` (`ApplicationName`, `ApplicationDescription`,
      `FileAssociations\.pdf = MegaUltraPDF.Document`) ;
    - `Software\RegisteredApplications\MegaUltraPDF`.
  - À la désinstallation, il les retire.
  - C'est ce qui permet au lien `ms-settings:defaultapps?registeredAppUser=MegaUltraPDF` de
    mener directement à la fiche de l'application.
  - L'installateur MSI garde l'association simple : décision consignée ; le bouton ouvre alors
    la page générale.
- Vérifier le nom du fichier `.desktop` du `.deb` et sa ligne
  `MimeType=application/pdf;`.

## Fichiers critiques

- `app/src-tauri/Cargo.toml`, `app/src-tauri/src/{lib.rs,commands.rs,default_app.rs}`
- `app/src-tauri/tauri.conf.json`, `app/src-tauri/windows/hooks.nsh`
- Capacités Tauri du greffon single-instance, s'il en exige :
  `app/src-tauri/capabilities/*.json`.
- `app/src/lib/{api.ts,strings.ts,logic/defaultapp.ts}`,
  `components/PreferencesDialog.svelte`, `routes/+page.svelte` (`startup_files`).
- Documentation :
  - ADR-0015 (une instance, lecteur par défaut, limites par système) ;
  - CLAUDE.md (pièges) ;
  - ROADMAP, FONCTIONNALITES ;
  - aide intégrée (section préférences) ;
  - `docs/apprentissage/lot-11.md`.

## Vérification

- `/verif` 8/8. Tests Rust des fonctions pures : arguments, analyse de `reg query` et de
  `xdg-mime`. Vitest de `logic/defaultapp.ts`.
- **Une instance** (Windows, manuel scripté en PowerShell) :
  1. lancer l'application en développement ;
  2. lancer une seconde fois l'exécutable avec `fixtures/simple.pdf` puis
     `fixtures/rotated.pdf` ;
  3. vérifier par CDP qu'il n'existe qu'une fenêtre et que deux onglets ont été ajoutés.

  Même contrôle sur l'application **installée** depuis le paquet NSIS de la release.
- **Lecteur par défaut sous Windows** :
  1. installer le paquet NSIS ;
  2. vérifier les clés `Capabilities` et `RegisteredApplications` (`reg query`) ;
  3. dans les préférences, le bouton ouvre la page des paramètres de l'application ;
  4. après le choix manuel, l'état affiché devient « lecteur par défaut » ;
  5. désinstaller : les clés disparaissent.
- Scénario CDP `lecteur-par-defaut` : la section des préférences s'affiche, avec un état et un
  bouton.
- CI sur 3 OS ; Linux et macOS : compilation et tests des fonctions pures seulement (pas de
  bureau en CI).
