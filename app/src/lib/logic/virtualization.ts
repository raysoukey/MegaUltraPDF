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

/** Une page à monter dans la visionneuse, avec sa priorité de rendu. */
export interface RenderSlot {
  index: number;
  priority: 'visible' | 'prefetch';
  interactive: boolean;
}

/**
 * Les pages à monter, **une entrée par page** : visibles d'abord (interactives), puis celles
 * préchargées. Une seule liste, clé = numéro de page : une page qui passe de « préchargée » à
 * « visible » garde son composant (pas de clignotement, A15).
 */
export function pageSlots(visible: number[], prefetch: number[]): RenderSlot[] {
  const seen = new Set(visible);
  return [
    ...visible.map((index) => ({ index, priority: 'visible' as const, interactive: true })),
    ...prefetch
      .filter((index) => !seen.has(index))
      .map((index) => ({ index, priority: 'prefetch' as const, interactive: false })),
  ];
}

/**
 * Molette en mode page unique (A16) : page suivante (`1`) quand on pousse vers le bas en
 * bas de la page, précédente (`-1`) vers le haut en haut ; sinon `0` (on défile la page).
 */
export function wheelPageStep(
  deltaY: number,
  scrollTop: number,
  scrollHeight: number,
  clientHeight: number,
): -1 | 0 | 1 {
  const atTop = scrollTop <= 1;
  const atBottom = scrollTop + clientHeight >= scrollHeight - 1;
  if (deltaY > 0 && atBottom) return 1;
  if (deltaY < 0 && atTop) return -1;
  return 0;
}
