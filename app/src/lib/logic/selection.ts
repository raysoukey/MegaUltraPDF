// SPDX-License-Identifier: GPL-3.0-or-later
// Sélection de texte sur le calque de caractères : une plage d'index dans l'ordre de
// lecture calculé par mega-core (`reading_order`, correctif 0.12.2). Tout est en points PDF
// (origine en bas à gauche).

import type { Rect, TextBox } from '$lib/api';

/** Distance d'une valeur à l'intervalle [a, b] : 0 à l'intérieur. */
function outside(v: number, a: number, b: number): number {
  return v < a ? a - v : v > b ? v - b : 0;
}

/** Deux distances verticales plus proches que cela (en points) désignent la même ligne. */
const LINE_TOLERANCE = 0.5;

/**
 * Index du caractère sous le point ; sinon celui de la **ligne la plus proche** (verticalement),
 * puis le plus proche horizontalement sur cette ligne ; `-1` sans caractères. Viser d'abord la
 * ligne évite de sauter sur une autre ligne (ou une autre colonne) quand le pointeur dépasse la
 * fin d'une ligne courte.
 */
export function charIndexAt(boxes: TextBox[], x: number, y: number): number {
  let best = -1;
  let bestDy = Number.POSITIVE_INFINITY;
  let bestDx = Number.POSITIVE_INFINITY;
  for (let i = 0; i < boxes.length; i++) {
    const r = boxes[i].rect;
    const dy = outside(y, r.y, r.y + r.h);
    const dx = outside(x, r.x, r.x + r.w);
    if (dx === 0 && dy === 0) return i;
    const nearerLine = dy < bestDy - LINE_TOLERANCE;
    const sameLine = Math.abs(dy - bestDy) <= LINE_TOLERANCE;
    if (nearerLine || (sameLine && dx < bestDx)) {
      best = i;
      bestDy = Math.min(dy, bestDy);
      bestDx = dx;
    }
  }
  return best;
}

/** Deux caractères sont sur la même ligne si leurs bords bas sont proches (demi-hauteur). */
function sameLine(a: Rect, b: Rect): boolean {
  return Math.abs(a.y - b.y) < Math.max(a.h, b.h) / 2;
}

function ordered(from: number, to: number): [number, number] {
  return from <= to ? [from, to] : [to, from];
}

/**
 * Le mot autour du caractère `i` (double-clic) : on s'étend tant que les voisins ne sont pas
 * des espaces **et** restent sur la même ligne (mega-core ne fournit plus les sauts de ligne de
 * PDFium, qui servaient de frontière).
 */
export function wordAt(boxes: TextBox[], i: number): [number, number] {
  const isWord = (k: number) => !/\s/.test(boxes[k].ch);
  if (!isWord(i)) return [i, i];
  let from = i;
  let to = i;
  while (from > 0 && isWord(from - 1) && sameLine(boxes[from - 1].rect, boxes[from].rect)) from--;
  while (to < boxes.length - 1 && isWord(to + 1) && sameLine(boxes[to + 1].rect, boxes[to].rect)) {
    to++;
  }
  return [from, to];
}

/** Rectangles à surligner pour la plage `[from, to]` (bornes incluses, ordre libre). */
export function selectionRects(boxes: TextBox[], from: number, to: number): Rect[] {
  const [start, end] = ordered(from, to);
  const rects: Rect[] = [];
  let current: Rect | null = null;
  for (let i = Math.max(0, start); i <= Math.min(end, boxes.length - 1); i++) {
    const r = boxes[i].rect;
    if (current && sameLine(current, r)) {
      const left = Math.min(current.x, r.x);
      const right = Math.max(current.x + current.w, r.x + r.w);
      const bottom = Math.min(current.y, r.y);
      const top = Math.max(current.y + current.h, r.y + r.h);
      current = { x: left, y: bottom, w: right - left, h: top - bottom };
    } else {
      if (current) rects.push(current);
      current = { ...r };
    }
  }
  if (current) rects.push(current);
  return rects;
}

/** Un vide vertical d'au moins cette part de la hauteur sépare deux paragraphes (comme mega-core). */
const PARAGRAPH_GAP = 0.8;

/**
 * Texte de la plage, mis en forme comme la page : un saut de ligne à chaque changement de
 * ligne, une ligne vide entre deux paragraphes, une espace entre deux mots d'une même ligne
 * séparés par un blanc sans caractère d'espace (un libellé et sa valeur).
 */
export function selectedText(boxes: TextBox[], from: number, to: number): string {
  const [start, end] = ordered(from, to);
  let text = '';
  let previous: TextBox | null = null;
  for (let i = Math.max(0, start); i <= Math.min(end, boxes.length - 1); i++) {
    const box = boxes[i];
    if (box.ch === '\n' || box.ch === '\r') continue;
    if (previous) {
      const a = previous.rect;
      const b = box.rect;
      const height = Math.max(a.h, b.h);
      if (!sameLine(a, b)) {
        const vgap = a.y - (b.y + b.h);
        text += vgap > PARAGRAPH_GAP * height ? '\n\n' : '\n';
      } else {
        const gap = b.x - (a.x + a.w);
        const spaced = /\s/.test(previous.ch) || /\s/.test(box.ch);
        if (!spaced && gap > 0.5 * height) text += ' ';
      }
    }
    text += box.ch;
    previous = box;
  }
  return text;
}
