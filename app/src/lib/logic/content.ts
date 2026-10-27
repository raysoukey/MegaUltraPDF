// SPDX-License-Identifier: GPL-3.0-or-later
// Logique pure des outils de contenu (Lot 5) : désignation d'un objet sous le pointeur,
// rectangles tracés, modèle de numérotation, portée « pages », validation du recadrage.
// Aucune décision sur le document : tout cela ne fait que préparer une commande Rust.

import type { PageObject, PageSize, Rect } from '$lib/api';
import type { Pt, Tool } from './tools';

/** Les outils qui agissent sur le contenu de la page (calque `ObjectLayer`). */
export type ContentTool = 'objects' | 'text-content' | 'image' | 'redact';

export function isContentTool(tool: Tool): tool is ContentTool {
  return tool === 'objects' || tool === 'text-content' || tool === 'image' || tool === 'redact';
}

/** Tolérance (points) autour d'un objet très fin (ligne, texte d'une ligne). */
const HIT_MARGIN = 2;

/** L'index de l'objet **le plus haut** (dessiné en dernier) sous `p`, ou `null`. */
export function hitTestObjects(objects: PageObject[], p: Pt): number | null {
  for (let i = objects.length - 1; i >= 0; i--) {
    const b = objects[i].bounds;
    if (
      p.x >= b.x - HIT_MARGIN &&
      p.x <= b.x + b.w + HIT_MARGIN &&
      p.y >= b.y - HIT_MARGIN &&
      p.y <= b.y + b.h + HIT_MARGIN
    )
      return objects[i].index;
  }
  return null;
}

/** Rectangle normalisé (largeur et hauteur positives) à partir de deux coins quelconques. */
export function rectFromCorners(a: Pt, b: Pt): Rect {
  return {
    x: Math.min(a.x, b.x),
    y: Math.min(a.y, b.y),
    w: Math.abs(a.x - b.x),
    h: Math.abs(a.y - b.y),
  };
}

/** Aperçu d'un modèle de numérotation (la substitution réelle est faite en Rust). */
export function fillTemplate(template: string, n: number, total: number, title: string): string {
  return template
    .replaceAll('{n}', String(n))
    .replaceAll('{total}', String(total))
    .replaceAll('{titre}', title);
}

export type PageScope = 'all' | 'current' | 'selected';

/** Liste de pages attendue par le backend : `null` = toutes ; triée, sans doublon. */
export function scopePages(scope: PageScope, current: number, selected: number[]): number[] | null {
  if (scope === 'all') return null;
  if (scope === 'selected' && selected.length > 0)
    return [...new Set(selected)].sort((a, b) => a - b);
  return [current];
}

/** Vrai si les marges `[gauche, bas, droite, haut]` laissent au moins 1 pt de page. */
export function cropMarginsValid(
  margins: [number, number, number, number],
  page: PageSize,
): boolean {
  if (margins.some((m) => !Number.isFinite(m) || m < 0)) return false;
  const [l, b, r, t] = margins;
  return page.width_pt - l - r >= 1 && page.height_pt - b - t >= 1;
}
