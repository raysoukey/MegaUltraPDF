// SPDX-License-Identifier: GPL-3.0-or-later
// Conversion entre les coordonnées PDF (points, origine en bas à gauche) et les
// coordonnées CSS d'une page affichée (pixels, origine en haut à gauche), avec rotation.

import type { PageSize, Rect, Rotation } from '$lib/api';

/** Rectangle CSS en pixels, relatif au coin haut-gauche de la page affichée. */
export interface CssRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** Dimensions (points) d'une page une fois tournée. */
export function rotatedSize(page: PageSize, rotation: Rotation): { w: number; h: number } {
  return rotation === 'cw90' || rotation === 'cw270'
    ? { w: page.height_pt, h: page.width_pt }
    : { w: page.width_pt, h: page.height_pt };
}

/**
 * Place un rectangle PDF sur la page affichée.
 *
 * Sans rotation, on inverse l'axe vertical : `top = H − (y + h)`. Les rotations
 * horaires font tourner le repère : le bord gauche de l'image tournée de 90° correspond
 * au bord bas de la page d'origine, etc. Les quatre cas ont été dérivés à la main et sont
 * fixés par les tests.
 */
export function pdfRectToCss(
  rect: Rect,
  page: PageSize,
  scale: number,
  rotation: Rotation,
): CssRect {
  const W = page.width_pt;
  const H = page.height_pt;
  let left: number;
  let top: number;
  let width: number;
  let height: number;
  switch (rotation) {
    case 'cw90':
      left = rect.y;
      top = rect.x;
      width = rect.h;
      height = rect.w;
      break;
    case 'cw180':
      left = W - rect.x - rect.w;
      top = rect.y;
      width = rect.w;
      height = rect.h;
      break;
    case 'cw270':
      left = H - rect.y - rect.h;
      top = W - rect.x - rect.w;
      width = rect.h;
      height = rect.w;
      break;
    default:
      left = rect.x;
      top = H - rect.y - rect.h;
      width = rect.w;
      height = rect.h;
  }
  return { left: left * scale, top: top * scale, width: width * scale, height: height * scale };
}

/** Inverse de `pdfRectToCss` pour un point : pixels CSS de la page → point PDF. */
export function cssPointToPdf(
  cssX: number,
  cssY: number,
  page: PageSize,
  scale: number,
  rotation: Rotation,
): { x: number; y: number } {
  const W = page.width_pt;
  const H = page.height_pt;
  const px = cssX / scale;
  const py = cssY / scale;
  switch (rotation) {
    case 'cw90':
      return { x: py, y: px };
    case 'cw180':
      return { x: W - px, y: py };
    case 'cw270':
      return { x: W - py, y: H - px };
    default:
      return { x: px, y: H - py };
  }
}
