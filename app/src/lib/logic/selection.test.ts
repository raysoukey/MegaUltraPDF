// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { charIndexAt, selectedText, selectionRects, wordAt } from './selection';
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

// Correctif 0.12.2 : la sélection suit les lignes ; les boîtes arrivent dans l'ordre de lecture.
/** Un mot posé en (x, y), une boîte de 10 × 12 pt par lettre. */
function word(text: string, x: number, y: number): TextBox[] {
  return [...text].map((ch, k) => ({ ch, rect: { x: x + 10 * k, y, w: 10, h: 12 } }));
}

describe('charIndexAt follows lines', () => {
  // Une ligne courte (« Pa ») au-dessus d'une ligne longue qui va jusqu'à x = 200.
  const lines = [...word('Pa', 10, 100), ...word('abcdefghijklmnopqrst', 10, 80)];
  it('stays on the line when the pointer goes past its end', () => {
    expect(charIndexAt(lines, 190, 105)).toBe(1);
  });
  it('picks the vertically nearest line between two lines', () => {
    expect(lines[charIndexAt(lines, 15, 94)].rect.y).toBe(80);
    expect(lines[charIndexAt(lines, 15, 98)].rect.y).toBe(100);
  });
  it('picks the nearer column in a gutter', () => {
    const cols = [...word('ab', 10, 100), ...word('cd', 100, 100)];
    expect(cols[charIndexAt(cols, 45, 105)].ch).toBe('b');
    expect(cols[charIndexAt(cols, 85, 105)].ch).toBe('c');
  });
});

describe('wordAt', () => {
  it('stops a word at the end of its line', () => {
    // « un mot » puis « suite » sur la ligne suivante, sans saut de ligne entre les deux.
    const b = [...word('un mot', 10, 100), ...word('suite', 10, 80)];
    expect(wordAt(b, 5)).toEqual([3, 5]);
    expect(wordAt(b, 6)).toEqual([6, 10]);
  });
});

describe('selectedText keeps the layout', () => {
  it('puts a space between a label and its value', () => {
    const b = [...word('Nom:', 10, 100), ...word('Alice', 200, 100)];
    expect(selectedText(b, 0, b.length - 1)).toBe('Nom: Alice');
  });
  it('separates paragraphs with a blank line', () => {
    const b = [...word('fin', 10, 100), ...word('debut', 10, 70)];
    expect(selectedText(b, 0, b.length - 1)).toBe('fin\n\ndebut');
  });
});
