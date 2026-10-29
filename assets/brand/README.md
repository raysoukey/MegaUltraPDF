# Visuels de MegaUltraPDF

Créés pour le projet par son propriétaire, avec ChatGPT, le 2026-10-26. Ce sont les **originaux** :
les images utilisées par l'application et le README en sont dérivées, jamais retouchées à la main.

| Fichier | Contenu | Original (dossier `images/`, non versionné) |
|---|---|---|
| `icon.png` | Icône carrée, fond transparent | `Image ChatGPT 26 oct. 2026, 20_53_23-2.png` |
| `logo.png` | Logo complet (icône et nom), fond transparent | `Image ChatGPT 26 oct. 2026, 20_55_08-1.png` |
| `actions.png` | Planche de 12 icônes d'actions avec légendes, fond transparent | `Image ChatGPT 26 oct. 2026, 20_53_23-3.png` |
| `banner-fr.png` | Bannière française | `Image ChatGPT 26 oct. 2026, 20_55_09-4.png` |

## Régénérer les images dérivées

```bash
cargo xtask brand                                # app/static/icons/*.png (sans légende), app/static/logo.png, docs/images/banniere.jpg
pnpm --dir app tauri icon ../assets/brand/icon.png   # app/src-tauri/icons/* (supprimer ensuite android/, ios/ et 64x64.png)
```
