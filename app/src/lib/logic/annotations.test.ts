// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import type { Annotation, Rect } from '$lib/api';
import {
  handlesFor,
  hitTest,
  noteIconRect,
  quadsFromSelection,
  resize,
  selectionAfterMove,
  toPdfRect,
} from './annotations';

function ann(index: number, rect: Rect, kind: Annotation['kind'] = 'rect'): Annotation {
  return {
    index,
    kind,
    rect,
    color: null,
    fill: null,
    opacity: 1,
    stroke_width: 1,
    font_size: null,
    author: null,
    contents: null,
    created: null,
    modified: null,
    quads: [],
    geometry: null,
    editable: true,
  };
}

describe('handlesFor / resize', () => {
  it('places four corner handles plus a move handle', () => {
    const h = handlesFor({ x: 10, y: 20, w: 100, h: 50 });
    expect(h.map((x) => x.id).sort()).toEqual(['move', 'ne', 'nw', 'se', 'sw']);
    expect(h.find((x) => x.id === 'ne')!.at).toEqual({ x: 110, y: 70 });
    expect(h.find((x) => x.id === 'sw')!.at).toEqual({ x: 10, y: 20 });
  });
  it('resizes from a corner and moves from the move handle, never below 4 pt', () => {
    const r = { x: 10, y: 20, w: 100, h: 50 };
    expect(resize(r, 'se', { dx: 10, dy: -5 })).toEqual({ x: 10, y: 15, w: 110, h: 55 });
    expect(resize(r, 'nw', { dx: 5, dy: -5 })).toEqual({ x: 15, y: 20, w: 95, h: 45 });
    expect(resize(r, 'move', { dx: -10, dy: 30 })).toEqual({ x: 0, y: 50, w: 100, h: 50 });
    const tiny = resize(r, 'se', { dx: -200, dy: 200 });
    expect(tiny.w).toBe(4);
    expect(tiny.h).toBe(4);
  });
});

describe('hitTest', () => {
  it('returns the topmost (latest) annotation under the point, or null', () => {
    const list = [ann(0, { x: 0, y: 0, w: 100, h: 100 }), ann(1, { x: 50, y: 50, w: 100, h: 100 })];
    expect(hitTest(list, { x: 75, y: 75 })).toBe(1);
    expect(hitTest(list, { x: 10, y: 10 })).toBe(0);
    expect(hitTest(list, { x: 300, y: 300 })).toBeNull();
  });
  it('ignores links and form fields', () => {
    const list = [ann(0, { x: 0, y: 0, w: 100, h: 100 }, 'link')];
    expect(hitTest(list, { x: 10, y: 10 })).toBeNull();
  });
});

describe('quadsFromSelection', () => {
  it('merges rectangles of the same line and keeps distinct lines', () => {
    const rects = [
      { x: 10, y: 100, w: 20, h: 10 },
      { x: 30, y: 101, w: 20, h: 10 },
      { x: 10, y: 80, w: 50, h: 10 },
    ];
    expect(quadsFromSelection(rects)).toEqual([
      { x: 10, y: 100, w: 40, h: 11 },
      { x: 10, y: 80, w: 50, h: 10 },
    ]);
  });
});

describe('noteIconRect', () => {
  it('is a 20 pt square anchored at the annotation rect top-left', () => {
    expect(noteIconRect(ann(0, { x: 50, y: 750, w: 20, h: 20 }, 'note'))).toEqual({
      x: 50,
      y: 750,
      w: 20,
      h: 20,
    });
  });
});

describe('toPdfRect', () => {
  const page = { width_pt: 100, height_pt: 200 };
  it('maps a CSS rect back to PDF coordinates without rotation', () => {
    expect(toPdfRect({ left: 10, top: 20, width: 30, height: 40 }, page, 1, 'none')).toEqual({
      x: 10,
      y: 140,
      w: 30,
      h: 40,
    });
  });
  it('maps a CSS rect in the top-left of a cw90 view to the top-right of the PDF page', () => {
    // Page affichée tournée de 90° : largeur CSS 200, hauteur CSS 100. Le coin haut-gauche
    // de la vue correspond au coin bas-gauche d'origine devenu... voir pdfRectToCss(cw90).
    const r = toPdfRect({ left: 0, top: 0, width: 20, height: 10 }, page, 1, 'cw90');
    expect(r).toEqual({ x: 0, y: 0, w: 10, h: 20 });
    const r2 = toPdfRect({ left: 180, top: 90, width: 20, height: 10 }, page, 2, 'cw90');
    expect(r2).toEqual({ x: 45, y: 90, w: 5, h: 10 });
  });
});

describe('selectionAfterMove', () => {
  it('keeps the index of a note and selects the last annotation for a recreated shape', () => {
    expect(selectionAfterMove('note', 1, 3)).toBe(1);
    expect(selectionAfterMove('rect', 0, 3)).toBe(2);
    expect(selectionAfterMove('text', 5, 1)).toBe(0);
  });
});
