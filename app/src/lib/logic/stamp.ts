// SPDX-License-Identifier: GPL-3.0-or-later
// Emplacement du tampon d'une signature visible (logique pure, testée).
import type { PageSize, Rect } from '$lib/api';

/** Coin de la page où poser le tampon. */
export type StampCorner = 'bottom_right' | 'bottom_left' | 'top_right' | 'top_left';

const WIDTH = 220;
const HEIGHT = 70;
const MARGIN = 36;
const MIN_SIDE = 10;

/**
 * Rectangle (points, espace affiché) d'un tampon de 220 × 70 pt à 36 pt du coin choisi ;
 * réduit sur une petite page pour rester dedans.
 */
export function stampRect(page: PageSize, corner: StampCorner): Rect {
  const margin = Math.min(MARGIN, page.width_pt * 0.1, page.height_pt * 0.1);
  const w = Math.max(MIN_SIDE, Math.min(WIDTH, page.width_pt - 2 * margin));
  const h = Math.max(MIN_SIDE, Math.min(HEIGHT, page.height_pt - 2 * margin));
  const left = corner === 'bottom_left' || corner === 'top_left';
  const bottom = corner === 'bottom_left' || corner === 'bottom_right';
  return {
    x: left ? margin : page.width_pt - margin - w,
    y: bottom ? margin : page.height_pt - margin - h,
    w,
    h,
  };
}
