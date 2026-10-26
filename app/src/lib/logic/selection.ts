// SPDX-License-Identifier: GPL-3.0-or-later
// Sélection de texte sur le calque de caractères : une plage d'index dans l'ordre de
// lecture fourni par PDFium. Tout est en points PDF (origine en bas à gauche).

import type { Rect, TextBox } from '$lib/api';

function contains(r: Rect, x: number, y: number): boolean {
  return x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h;
}

function distance2(r: Rect, x: number, y: number): number {
  const cx = r.x + r.w / 2;
  const cy = r.y + r.h / 2;
  return (cx - x) ** 2 + (cy - y) ** 2;
}

/** Index du caractère sous le point, sinon le plus proche ; `-1` sans caractères. */
export function charIndexAt(boxes: TextBox[], x: number, y: number): number {
  let best = -1;
  let bestDistance = Number.POSITIVE_INFINITY;
  for (let i = 0; i < boxes.length; i++) {
    const r = boxes[i].rect;
    if (contains(r, x, y)) return i;
    const d = distance2(r, x, y);
    if (d < bestDistance) {
      bestDistance = d;
      best = i;
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

/** Texte de la plage : caractères dans l'ordre, un saut de ligne à chaque changement de ligne. */
export function selectedText(boxes: TextBox[], from: number, to: number): string {
  const [start, end] = ordered(from, to);
  let text = '';
  let previous: Rect | null = null;
  for (let i = Math.max(0, start); i <= Math.min(end, boxes.length - 1); i++) {
    const { ch, rect } = boxes[i];
    if (previous && !sameLine(previous, rect) && !text.endsWith('\n')) text += '\n';
    if (ch !== '\n' && ch !== '\r') text += ch;
    previous = rect;
  }
  return text;
}
