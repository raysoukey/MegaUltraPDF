// SPDX-License-Identifier: GPL-3.0-or-later
// Disposition verticale des pages et calcul des pages visibles : on ne demande au
// backend que les images qui peuvent être vues (plus quelques-unes autour).

import type { PageSize, Rotation } from '$lib/api';
import { rotatedSize } from './geometry';

/** Position et taille (pixels) d'une page dans la pile verticale. */
export interface PageSlot {
  top: number;
  width: number;
  height: number;
}

/** Empile les pages avec un écart `gap` (pixels) entre elles. */
export function pageLayout(
  pages: PageSize[],
  scale: number,
  rotation: Rotation,
  gap: number,
): PageSlot[] {
  const slots: PageSlot[] = [];
  let top = 0;
  for (const page of pages) {
    const { w, h } = rotatedSize(page, rotation);
    const width = w * scale;
    const height = h * scale;
    slots.push({ top, width, height });
    top += height + gap;
  }
  return slots;
}

/** Index des pages qui croisent la fenêtre `[scrollTop, scrollTop + viewportHeight]`, plus `overscan` pages de chaque côté. */
export function visiblePages(
  layout: PageSlot[],
  scrollTop: number,
  viewportHeight: number,
  overscan: number,
): number[] {
  if (layout.length === 0) return [];
  const bottom = scrollTop + viewportHeight;
  let first = -1;
  let last = -1;
  for (let i = 0; i < layout.length; i++) {
    const slot = layout[i];
    const intersects = slot.top < bottom && slot.top + slot.height > scrollTop;
    if (intersects) {
      if (first === -1) first = i;
      last = i;
    }
  }
  if (first === -1) {
    // Fenêtre au-delà de la dernière page (ou avant la première) : on prend la plus proche.
    first = last = scrollTop >= layout[layout.length - 1].top ? layout.length - 1 : 0;
  }
  const from = Math.max(0, first - overscan);
  const to = Math.min(layout.length - 1, last + overscan);
  const result: number[] = [];
  for (let i = from; i <= to; i++) result.push(i);
  return result;
}

/** Page « courante » : celle qui contient le point de lecture (tiers supérieur de la fenêtre). */
export function pageAtScroll(
  layout: PageSlot[],
  scrollTop: number,
  viewportHeight: number,
): number {
  if (layout.length === 0) return 0;
  // En butée basse, la dernière page est la page courante même si elle ne peut pas
  // atteindre le tiers supérieur (document court, faible zoom).
  const last = layout[layout.length - 1];
  if (scrollTop + viewportHeight >= last.top + last.height - 1) return layout.length - 1;
  const probe = scrollTop + viewportHeight / 3;
  for (let i = 0; i < layout.length; i++) {
    const slot = layout[i];
    if (probe < slot.top + slot.height) return i;
  }
  return layout.length - 1;
}
