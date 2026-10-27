// SPDX-License-Identifier: GPL-3.0-or-later
// Géométrie d'interface des annotations : poignées, redimensionnement, test de survol,
// quadrilatères depuis une sélection de texte, conversions écran → PDF. Tout en points PDF
// sauf mention contraire.

import type { Annotation, AnnotationKind, PageSize, Rect, Rotation } from '$lib/api';
import { cssPointToPdf, type CssRect } from './geometry';
import type { Pt } from './tools';

export type HandleId = 'nw' | 'ne' | 'sw' | 'se' | 'move';

export interface Handle {
  id: HandleId;
  at: Pt;
}

/** Côté de l'icône d'une note (même valeur que `NOTE_ICON_SIZE` côté Rust). */
export const NOTE_ICON_SIZE = 20;
const MIN_SIZE = 4;

export function handlesFor(r: Rect): Handle[] {
  return [
    { id: 'nw', at: { x: r.x, y: r.y + r.h } },
    { id: 'ne', at: { x: r.x + r.w, y: r.y + r.h } },
    { id: 'sw', at: { x: r.x, y: r.y } },
    { id: 'se', at: { x: r.x + r.w, y: r.y } },
    { id: 'move', at: { x: r.x + r.w / 2, y: r.y + r.h / 2 } },
  ];
}

/** Applique un déplacement (PDF, y vers le haut) à une poignée ; taille minimale 4 pt. */
export function resize(r: Rect, handle: HandleId, delta: { dx: number; dy: number }): Rect {
  let { x, y, w, h } = r;
  switch (handle) {
    case 'move':
      x += delta.dx;
      y += delta.dy;
      break;
    case 'se':
      w += delta.dx;
      y += delta.dy;
      h -= delta.dy;
      break;
    case 'sw':
      x += delta.dx;
      w -= delta.dx;
      y += delta.dy;
      h -= delta.dy;
      break;
    case 'ne':
      w += delta.dx;
      h += delta.dy;
      break;
    case 'nw':
      x += delta.dx;
      w -= delta.dx;
      h += delta.dy;
      break;
  }
  return { x, y, w: Math.max(MIN_SIZE, w), h: Math.max(MIN_SIZE, h) };
}

function contains(r: Rect, p: Pt): boolean {
  return p.x >= r.x && p.x <= r.x + r.w && p.y >= r.y && p.y <= r.y + r.h;
}

/** Emprise cliquable : une note se saisit par son icône. */
function hitRect(a: Annotation): Rect {
  return a.kind === 'note' ? noteIconRect(a) : a.rect;
}

/** Index de l'annotation sous le point (la plus récente d'abord), ou `null`. */
export function hitTest(annotations: Annotation[], p: Pt): number | null {
  for (let i = annotations.length - 1; i >= 0; i--) {
    const a = annotations[i];
    if (a.kind === 'link' || a.kind === 'widget') continue;
    if (contains(hitRect(a), p)) return a.index;
  }
  return null;
}

function sameLine(a: Rect, b: Rect): boolean {
  const ca = a.y + a.h / 2;
  const cb = b.y + b.h / 2;
  return Math.abs(ca - cb) < Math.min(a.h, b.h) / 2;
}

/** Fusionne les rectangles d'une même ligne (quadrilatères d'un marquage de texte). */
export function quadsFromSelection(rects: Rect[]): Rect[] {
  const out: Rect[] = [];
  for (const r of rects) {
    const last = out[out.length - 1];
    if (last && sameLine(last, r)) {
      const left = Math.min(last.x, r.x);
      const bottom = Math.min(last.y, r.y);
      const right = Math.max(last.x + last.w, r.x + r.w);
      const top = Math.max(last.y + last.h, r.y + r.h);
      out[out.length - 1] = { x: left, y: bottom, w: right - left, h: top - bottom };
    } else {
      out.push({ ...r });
    }
  }
  return out;
}

/** Icône d'une note : carré de 20 pt ancré au coin haut-gauche de son `/Rect`. */
export function noteIconRect(a: Annotation): Rect {
  return {
    x: a.rect.x,
    y: a.rect.y + a.rect.h - NOTE_ICON_SIZE,
    w: NOTE_ICON_SIZE,
    h: NOTE_ICON_SIZE,
  };
}

/** Rectangle CSS de la page affichée → rectangle PDF, quelle que soit la rotation. */
export function toPdfRect(css: CssRect, page: PageSize, scale: number, rotation: Rotation): Rect {
  const a = cssPointToPdf(css.left, css.top, page, scale, rotation);
  const b = cssPointToPdf(css.left + css.width, css.top + css.height, page, scale, rotation);
  const x = Math.min(a.x, b.x);
  const y = Math.min(a.y, b.y);
  return { x, y, w: Math.abs(a.x - b.x), h: Math.abs(a.y - b.y) };
}

/** Index à sélectionner après un déplacement : une note garde le sien, une forme recréée
 *  est la dernière de la page. */
export function selectionAfterMove(kind: AnnotationKind, index: number, count: number): number {
  return kind === 'note' ? index : Math.max(0, count - 1);
}
