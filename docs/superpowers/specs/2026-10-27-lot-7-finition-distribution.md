# Lot 7 — Finition et distribution : spécification

Statut : approuvée par mandat (exécution autonome). Référence : conception §5 Lot 7, ADR-0002
(interface Tauri), ADR-0004 (PDFium global), CLAUDE.md (chaînes centralisées, pas de réseau).

## Intention

Rendre l'application utilisable par d'autres que son auteur : en français **et** en anglais,
au clavier et avec un lecteur d'écran, réglable depuis un vrai dialogue de préférences, avec
une aide intégrée et une page « À propos » qui crédite les composants tiers, et **installable**
sur Windows, Linux et macOS à partir d'artefacts produits par la CI.

## Décision de périmètre (ruling)

Les deux options « désactivées par défaut » de la conception (OCR, signature numérique
cryptographique) forment un **Lot 8** distinct : chacune touche des dépendances natives ou
cryptographiques et mérite sa propre relecture. Le Lot 7 garde tout le reste du §5.

## Périmètre

Entre :
1. **Interface FR / EN.** `strings.ts` porte deux dictionnaires de même forme (`fr` référence,
   `en` vérifié par le typage) ; langue « système / français / anglais » dans les préférences ;
   au démarrage, la langue est connue de façon synchrone (miroir `localStorage`), donc aucun
   affichage dans la mauvaise langue ; le changement de langue remonte l'interface (bloc
   `{#key}`) sans fermer les documents. Formats localisés (octets « Ko/KB », nombres). Les
   messages d'erreur détaillés du cœur Rust restent en français (limite documentée) ; les
   erreurs connues (mot de passe, fichier introuvable…) sont traduites par leur `kind`.
2. **Accessibilité.** Menus « Fichier » et « Contenu » au clavier (Entrée / Espace / ↓ pour
   ouvrir, ↑ ↓ Début Fin pour naviguer, Échap pour fermer et rendre le focus au bouton, clic
   extérieur), `role="none"` sur les éléments de liste ; **piège de focus** dans tous les
   dialogues modaux et la vue d'impression (Tab / Maj+Tab bouclent, le focus revient à
   l'élément d'origine à la fermeture) ; tous les dialogues annoncés `role="dialog"
   aria-modal="true"` (les `<form>` sont enveloppés) ; `aria-label` sur tous les boutons à
   glyphe ; contour de focus visible (`:focus-visible`) ; contrastes du texte atténué ≥ 4,5:1
   dans les deux thèmes (vérifiés par un test sur les jetons de couleur).
3. **Préférences.** Dialogue (Fichier ▸ Préférences…, Ctrl+,) : langue, thème, mode de zoom par
   défaut, défilement continu ou page unique, nom d'auteur, « rouvrir à la dernière position »
   (nouveau réglage), vider la liste des récents.
4. **Aide intégrée.** F1 ouvre l'aide (onglets « Prise en main », « Raccourcis », « À propos ») ;
   la prise en main décrit en quelques sections les tâches courantes (ouvrir, annoter, remplir,
   éditer, exporter, protéger) ; la table des raccourcis est générée depuis `shortcuts.ts`.
5. **À propos et licences.** Version de l'application (Tauri `getVersion`), licence
   GPL-3.0-or-later, mention de PDFium (Google, BSD-3-Clause) ; liste des dépendances tierces
   (crates et paquets npm, licence et version) générée par `cargo xtask licenses` dans
   `app/src/lib/generated/licenses.json` (commité, regénéré à chaque version) et
   `THIRD_PARTY_LICENSES.md` à la racine ; le texte de licence de PDFium est extrait de
   l'archive par `fetch-pdfium` et livré avec l'application.
6. **Distribution.**
   - `engine::set_library_dir(path)` : l'application indique où chercher PDFium (dossier des
     ressources Tauri) avant le premier usage ; ordre de recherche : réglé par l'application,
     `MEGA_PDFIUM_DIR`, dossier de l'exécutable, `vendor/` en développement.
   - `cargo xtask stage-pdfium` copie la bibliothèque PDFium de la cible et sa licence dans
     `app/src-tauri/resources/pdfium/` (ignoré par git) ; `beforeBuildCommand` l'appelle ;
     `bundle.resources` les embarque.
   - Cibles de paquet explicites : NSIS et MSI (Windows), deb et AppImage (Linux), dmg (macOS) ;
     version portable Windows (zip : exécutable, `pdfium.dll`, licences).
   - Profil `release` réduit (LTO, `codegen-units = 1`, `strip`, `panic` inchangé).
   - Workflow `release.yml` (étiquette `v*` ou déclenchement manuel) : construit les paquets
     sur les trois OS, le zip portable, et les joint à une release GitHub **brouillon**.
     Paquets non signés (documenté : avertissement SmartScreen / Gatekeeper).
   - Version 0.7.0 (workspace Cargo, `tauri.conf.json`, `package.json`).
7. **Vérification de bout en bout du paquet** : installeur NSIS construit localement, installé
   en silence dans un dossier temporaire, application installée lancée avec le port de débogage,
   scénario `lecteur-simple.json` déroulé contre elle (PDFium trouvé dans les ressources).

N'entre pas : OCR et signature numérique (Lot 8), signature de code des installeurs,
mise à jour automatique (anti-objectif : pas d'appel réseau), traduction des messages d'erreur
détaillés du cœur, autres langues que FR/EN, paquets rpm / Flatpak / Snap.

## API ajoutées

- `mega-core` : `pub fn engine::set_library_dir(dir: PathBuf) -> bool` (vrai si pris en
  compte, faux si PDFium est déjà chargé ou le dossier déjà réglé).
- `xtask` : `licenses` (lit `cargo metadata` et `pnpm licenses list --json`), `stage-pdfium`.
- Tauri : réglage du dossier PDFium dans `setup` ; aucune nouvelle commande métier.
- Frontend : `logic/i18n.ts` (`resolveLanguage`), `logic/menu.ts` (navigation clavier),
  `logic/focus.ts` (piège de focus : indice suivant), `logic/contrast.ts` (rapport de
  contraste WCAG), action `trapFocus`, composants `PreferencesDialog`, `HelpDialog`.

## Concepts Rust travaillés

`OnceLock` réutilisé pour un réglage écrit une fois (`set_library_dir`), profils Cargo
(`[profile.release]`), `cargo metadata` et `serde_json` dans `xtask` (lire un JSON imbriqué
avec des structures partielles), chemins de ressources et différences entre OS
(`resource_dir`), `#[cfg(target_os = …)]`.

## Fixtures

Aucune nouvelle fixture PDF. `licenses.json` est un artefact généré et commité.

## Checklist de validation (CDP `finition.json` + paquet)

1. Préférences : passer en anglais → barre d'outils et menus en anglais, onglet du document
   toujours ouvert ; revenir au français.
2. Menu Fichier au clavier : focus sur le bouton, ↓ ouvre et sélectionne le premier élément,
   ↓ ↓ Échap ferme et rend le focus au bouton.
3. Dialogue Propriétés : Tab depuis le dernier contrôle revient au premier (piège de focus) ;
   Échap ferme et rend le focus au déclencheur.
4. F1 : aide avec trois onglets ; « À propos » affiche la version et PDFium.
5. Tous les boutons à glyphe ont un nom accessible (assertion sur le DOM).
6. Paquet : installeur NSIS installé en silence, application installée ouverte sur
   `simple.pdf`, scénario `lecteur-simple.json` vert.
