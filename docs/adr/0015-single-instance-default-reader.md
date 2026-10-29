# ADR-0015 — Une seule instance et lecteur PDF par défaut

Date : 2026-10-28 · Statut : accepté (Lot 11)

## Contexte

Les installateurs déclaraient déjà l'association `.pdf`, mais :
- ouvrir un PDF pendant que l'application tournait lançait une **seconde fenêtre** ;
- l'application ne disait pas si elle était le lecteur par défaut, ni comment le devenir ;
- le rôle déclaré était « Viewer » alors que l'application édite les PDF.

## Décisions

1. **Une seule instance** : greffon officiel `tauri-plugin-single-instance`, enregistré en
   premier, sous Windows et Linux. La seconde instance confie ses arguments à la première,
   puis s'arrête. Pas sous macOS : le système y garde déjà une seule instance, et la socket
   du greffon dans `/tmp` serait partagée entre utilisateurs.
   - `pdf_paths_from_args` en extrait les PDF : chemins absolus, relatifs lus dans le dossier
     de la seconde instance, sans options ni doublons.
   - La première instance range ces chemins dans `PendingPaths` et émet `open-paths`, le même
     mécanisme que le Finder sous macOS (Lot 9) ; l'interface les ouvre en onglets.
   - Au démarrage, **tous** les PDF passés en arguments sont ouverts (`startup_files`).
2. **Lecteur par défaut : selon ce que chaque système permet** (`default_app.rs`). C'est une
   intégration au système, pas de la logique PDF : elle vit dans l'application.
   - **Windows** : lecture de `UserChoiceLatest\ProgId` (Windows 11), sinon
     `UserChoice\ProgId`, par l'API du registre (crate `winreg`, déjà présent via Tauri) :
     la sortie de `reg.exe` suit la page de code de la console (« (par défaut) », chemins
     accentués) et ne se lit pas en UTF-8 (correctif 0.11.2). Windows 10/11
     interdit l'auto-désignation, alors le bouton ouvre la fiche de l'application dans les
     Paramètres (`ms-settings:defaultapps?registeredAppUser=MegaUltraPDF`) si l'installateur
     l'a enregistrée, la page générale sinon.
   - **Linux** : `xdg-mime query` et `xdg-mime default MegaUltraPDF.desktop application/pdf`,
     seulement si ce fichier `.desktop` est installé (dossiers XDG), puis relecture.
   - **macOS** : état inconnu (le lire demanderait une FFI et une dépendance) ; le bouton
     affiche la marche à suivre dans le Finder.
   - **Versions portables** : sous Windows, la commande d'ouverture du ProgId (HKCU puis
     HKLM, écrite par NSIS comme par MSI) ne désigne pas cet exécutable ; sous Linux, une
     AppImage. Signalées, sans bouton.
   - Les programmes interrogés tournent hors du fil de la fenêtre (`spawn_blocking`) et sont
     arrêtés après 3 secondes.
3. **Paquets** : rôle `Editor`. Le greffon NSIS `windows/hooks.nsh` écrit à l'installation
   `Software\MegaUltraPDF\Capabilities` et `Software\RegisteredApplications\MegaUltraPDF`, et
   les retire à la désinstallation. Sans cet enregistrement, la fiche de l'application
   n'existe pas dans les Paramètres. L'installateur MSI garde l'association simple ; le bouton
   ouvre alors la page générale.

## Conséquences

- Plusieurs fichiers choisis dans l'Explorateur ou le gestionnaire de fichiers arrivent dans
  une seule fenêtre.
- Le choix final reste à l'utilisateur sous Windows et macOS, par conception de ces systèmes.
