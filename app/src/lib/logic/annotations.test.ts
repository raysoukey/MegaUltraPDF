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

describe('selectionAfterMove', () => {
  it('keeps the index of a note and selects the last annotation for a recreated shape', () => {
    expect(selectionAfterMove('note', 1, 3)).toBe(1);
    expect(selectionAfterMove('rect', 0, 3)).toBe(2);
    expect(selectionAfterMove('text', 5, 1)).toBe(0);
  });
});
