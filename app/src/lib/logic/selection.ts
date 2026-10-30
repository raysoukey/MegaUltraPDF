// SPDX-License-Identifier: GPL-3.0-or-later
// Sélection de texte sur le calque de caractères : une plage d'index dans l'ordre de
// lecture calculé par mega-core (`reading_order`, correctif 0.12.2). Chaque caractère porte son
// numéro de ligne et de bloc : l'interface ne devine plus la mise en page avec la géométrie.
// Tout est en points PDF (origine en bas à gauche).

import type { Rect, TextBox } from '$lib/api';

/** Distance d'une valeur à l'intervalle [a, b] : 0 à l'intérieur. */
function outside(v: number, a: number, b: number): number {
  return v < a ? a - v : v > b ? v - b : 0;
}

/** Deux distances verticales plus proches que cela (en points) désignent la même ligne. */
const LINE_TOLERANCE = 0.5;

/** Emprise de chaque bloc (union des rectangles de ses caractères). */
function blockBounds(boxes: TextBox[]): Map<number, Rect> {
  const bounds = new Map<number, Rect>();
  for (const { block, rect: r } of boxes) {
    const b = bounds.get(block);
    if (!b) {
      bounds.set(block, { ...r });
    } else {
      const right = Math.max(b.x + b.w, r.x + r.w);
      const top = Math.max(b.y + b.h, r.y + r.h);
      b.x = Math.min(b.x, r.x);
      b.y = Math.min(b.y, r.y);
      b.w = right - b.x;
      b.h = top - b.y;
    }
  }
  return bounds;
}

/**
 * Index du caractère sous le point ; sinon on choisit d'abord le **bloc** le plus proche
 * (paragraphe, colonne), puis dans ce bloc la **ligne** la plus proche verticalement, puis le
 * caractère le plus proche horizontalement ; `-1` sans caractères. Ainsi le pointeur ne saute ni
 * sur une autre ligne quand il dépasse la fin d'une ligne courte, ni dans l'autre colonne quand
 * il passe dans le blanc entre deux paragraphes.
 */
export function charIndexAt(boxes: TextBox[], x: number, y: number): number {
  for (let i = 0; i < boxes.length; i++) {
    const r = boxes[i].rect;
    if (outside(x, r.x, r.x + r.w) === 0 && outside(y, r.y, r.y + r.h) === 0) return i;
  }
  let nearestBlock = -1;
  let blockDistance = Number.POSITIVE_INFINITY;
  for (const [block, b] of blockBounds(boxes)) {
    const d = Math.hypot(outside(x, b.x, b.x + b.w), outside(y, b.y, b.y + b.h));
    if (d < blockDistance) {
      blockDistance = d;
      nearestBlock = block;
    }
  }
  let best = -1;
  let bestDy = Number.POSITIVE_INFINITY;
  let bestDx = Number.POSITIVE_INFINITY;
  for (let i = 0; i < boxes.length; i++) {
    if (boxes[i].block !== nearestBlock) continue;
    const r = boxes[i].rect;
    const dy = outside(y, r.y, r.y + r.h);
    const dx = outside(x, r.x, r.x + r.w);
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

function ordered(from: number, to: number): [number, number] {
  return from <= to ? [from, to] : [to, from];
}

/**
 * Le mot autour du caractère `i` (double-clic) : on s'étend tant que les voisins ne sont pas
 * des espaces **et** restent sur la même ligne.
 */
export function wordAt(boxes: TextBox[], i: number): [number, number] {
  const isWord = (k: number) => !/\s/.test(boxes[k].ch);
  if (!isWord(i)) return [i, i];
  let from = i;
  let to = i;
  while (from > 0 && isWord(from - 1) && boxes[from - 1].line === boxes[i].line) from--;
  while (to < boxes.length - 1 && isWord(to + 1) && boxes[to + 1].line === boxes[i].line) to++;
  return [from, to];
}

/** Rectangles à surligner pour la plage `[from, to]` (bornes incluses, ordre libre) : un par ligne. */
export function selectionRects(boxes: TextBox[], from: number, to: number): Rect[] {
  const [start, end] = ordered(from, to);
  const rects: Rect[] = [];
  let current: Rect | null = null;
  let currentLine = -1;
  for (let i = Math.max(0, start); i <= Math.min(end, boxes.length - 1); i++) {
    const { rect: r, line } = boxes[i];
    if (current && line === currentLine) {
      const left = Math.min(current.x, r.x);
      const right = Math.max(current.x + current.w, r.x + r.w);
      const bottom = Math.min(current.y, r.y);
      const top = Math.max(current.y + current.h, r.y + r.h);
      current = { x: left, y: bottom, w: right - left, h: top - bottom };
    } else {
      if (current) rects.push(current);
      current = { ...r };
      currentLine = line;
    }
  }
  if (current) rects.push(current);
  return rects;
}

/**
 * Texte de la plage, mis en forme comme l'export texte de mega-core : un saut de ligne à chaque
 * changement de ligne, une ligne vide à chaque changement de bloc, une espace entre deux mots
 * d'une même ligne séparés par un blanc sans caractère d'espace (un libellé et sa valeur).
 */
export function selectedText(boxes: TextBox[], from: number, to: number): string {
  const [start, end] = ordered(from, to);
  let text = '';
  let previous: TextBox | null = null;
  for (let i = Math.max(0, start); i <= Math.min(end, boxes.length - 1); i++) {
    const box = boxes[i];
    if (previous) {
      if (box.block !== previous.block) {
        text += '\n\n';
      } else if (box.line !== previous.line) {
        text += '\n';
      } else {
        const gap = box.rect.x - (previous.rect.x + previous.rect.w);
        const spaced = /\s/.test(previous.ch) || /\s/.test(box.ch);
        if (!spaced && gap > 0.5 * Math.max(previous.rect.h, box.rect.h)) text += ' ';
      }
    }
    text += box.ch;
    previous = box;
  }
  return text;
}
