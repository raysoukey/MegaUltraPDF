# ADR-0002 — Interface en Tauri 2 + SvelteKit (Svelte 5, TypeScript)

Date : 2026-10-26 · Statut : accepté

## Contexte

Le premier brouillon de conception excluait toute pile web « par principe » pour rester
100 % Rust. L'utilisateur a explicitement demandé de **ne pas exclure une pile web si elle
optimise le projet**, et a choisi Tauri 2 parmi trois options présentées (Tauri + TypeScript,
Dioxus, egui).

## Décision

- Backend Rust Tauri 2 (`app/src-tauri`, crate `mega-app`) : fine couche de commandes
  `invoke` + protocole `mega://` servant les pages rendues en PNG + registre des documents.
- Frontend SvelteKit en mode SPA (`adapter-static`, `ssr = false`), Svelte 5 avec runes,
  TypeScript strict, Vite, Vitest, ESLint, Prettier. Généré par le gabarit officiel
  `create-tauri-app --template svelte-ts`.
- Transfert des pages par protocole personnalisé (`convertFileSrc(path, 'mega')`) plutôt que
  par `invoke` avec des octets en base64.
- Toutes les chaînes visibles dans `app/src/lib/strings.ts` dès le départ.

## Pourquoi

Une interface de lecteur/éditeur PDF profite directement du web : calques SVG pour la sélection
de texte et les annotations, champs HTML pour les formulaires, impression via le WebView,
accessibilité et i18n matures. Tauri utilise le WebView2 déjà présent dans Windows (pas de
Chromium embarqué), binaire léger, zéro télémétrie. Environ 60 % du code (toute la logique
PDF) reste en Rust, qui demeure l'objectif d'apprentissage.

## Alternatives écartées

- **egui** : tout Rust, simple, mais sélection de texte, formulaires et accessibilité plus
  pauvres.
- **Dioxus** : tout Rust sur WebView, mais écosystème et ressources d'apprentissage plus jeunes.
- **Electron** : Chromium embarqué, contraire à l'esprit « sans bloat ».

## Conséquences

- Deux langages dans le dépôt ; les notes d'apprentissage restent centrées sur Rust, le
  TypeScript est expliqué à la demande.
- La CSP de `tauri.conf.json` doit autoriser `mega:` et `http://mega.localhost` en `img-src`.
- Le frontend ne contient que de la présentation ; toute décision est une commande Rust.
