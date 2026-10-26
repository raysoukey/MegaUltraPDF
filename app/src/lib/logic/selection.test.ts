// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { charIndexAt, selectedText, selectionRects } from './selection';
import type { TextBox } from '$lib/api';

// « Pa » sur une ligne (y = 100), « ge » sur la suivante (y = 80), boîtes de 10 × 12 pt.
const boxes: TextBox[] = [
  { ch: 'P', rect: { x: 10, y: 100, w: 10, h: 12 } },
  { ch: 'a', rect: { x: 20, y: 100, w: 10, h: 12 } },
  { ch: 'g', rect: { x: 10, y: 80, w: 10, h: 12 } },
  { ch: 'e', rect: { x: 20, y: 80, w: 10, h: 12 } },
];

describe('charIndexAt', () => {
  it('returns the box containing the point', () => {
    expect(charIndexAt(boxes, 25, 105)).toBe(1);
    expect(charIndexAt(boxes, 12, 85)).toBe(2);
  });
  it('falls back to the nearest box', () => {
    expect(charIndexAt(boxes, 60, 106)).toBe(1);
    expect(charIndexAt([], 0, 0)).toBe(-1);
  });
});

describe('selectionRects', () => {
  it('merges consecutive characters on the same line', () => {
    expect(selectionRects(boxes, 0, 3)).toEqual([
      { x: 10, y: 100, w: 20, h: 12 },
      { x: 10, y: 80, w: 20, h: 12 },
    ]);
  });
  it('accepts a reversed range', () => {
    expect(selectionRects(boxes, 1, 0)).toEqual([{ x: 10, y: 100, w: 20, h: 12 }]);
  });
});

describe('selectedText', () => {
  it('joins characters and breaks lines', () => {
    expect(selectedText(boxes, 0, 3)).toBe('Pa\nge');
    expect(selectedText(boxes, 1, 2)).toBe('a\ng');
  });
});
