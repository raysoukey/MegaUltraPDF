// SPDX-License-Identifier: GPL-3.0-or-later
// Icônes d'actions (Lot 12) : images sans légende produites par `cargo xtask brand` à partir
// de la planche de `assets/brand/`, servies depuis `app/static/icons/`.

/** Les 12 icônes, dans l'ordre de la planche et de `ICON_NAMES` de `xtask/src/brand.rs`. */
export const ICON_NAMES = [
  'open',
  'save',
  'export',
  'merge',
  'split',
  'rotate',
  'annotate',
  'sign',
  'ocr',
  'search',
  'protect',
  'compress',
] as const;

/** Nom d'une icône : seules les 12 existantes sont acceptées par le compilateur. */
export type IconName = (typeof ICON_NAMES)[number];

/** Adresse de l'image d'une icône (fichier local de l'application, aucun réseau). */
export function iconSrc(name: IconName): string {
  return `/icons/${name}.png`;
}
