// SPDX-License-Identifier: GPL-3.0-or-later
// Calculs de zoom : paliers, bornes, « ajuster à la largeur / à la page ».

import type { PageSize } from '$lib/api';

export const MIN_SCALE = 0.1;
export const MAX_SCALE = 8;
export const ZOOM_STEPS = [
  0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4, 6, 8,
];

/** Ramène une échelle dans les bornes ; une valeur invalide vaut 1. */
export function clampScale(scale: number): number {
  if (!Number.isFinite(scale)) return 1;
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, scale));
}

/** Palier suivant (`dir = 1`) ou précédent (`dir = -1`) strictement au-delà de `current`. */
export function nextZoom(current: number, dir: 1 | -1): number {
  const eps = 1e-6;
  if (dir === 1) {
    const next = ZOOM_STEPS.find((s) => s > current + eps);
    return next ?? MAX_SCALE;
  }
  const prev = [...ZOOM_STEPS].reverse().find((s) => s < current - eps);
  return prev ?? MIN_SCALE;
}

/** Échelle pour que la page occupe toute la largeur disponible (moins les marges). */
export function fitWidthScale(page: PageSize, containerWidthPx: number, gutterPx: number): number {
  return clampScale((containerWidthPx - 2 * gutterPx) / page.width_pt);
}

/** Échelle pour que la page entière tienne dans le conteneur. */
export function fitPageScale(
  page: PageSize,
  containerWidthPx: number,
  containerHeightPx: number,
  gutterPx: number,
): number {
  const byWidth = (containerWidthPx - 2 * gutterPx) / page.width_pt;
  const byHeight = (containerHeightPx - 2 * gutterPx) / page.height_pt;
  return clampScale(Math.min(byWidth, byHeight));
}

/**
 * Défilement qui garde **sous le curseur** le même point du contenu après un zoom de facteur
 * `ratio` (A14, Lot 9). `cursor` : position du curseur dans la fenêtre ; `fixed` : la part du
 * contenu avant ce point qui ne grandit pas (écarts entre les pages, marges). Jamais négatif.
 */
export function anchoredScroll(
  scroll: number,
  cursor: number,
  ratio: number,
  fixed: number,
): number {
  return Math.max(0, (scroll + cursor - fixed) * ratio + fixed - cursor);
}
