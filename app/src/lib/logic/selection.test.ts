// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { charIndexAt, selectedText, selectionRects, wordAt } from './selection';
import type { TextBox } from '$lib/api';

/**
 * Un mot posé en (x, y), une boîte de 10 × 12 pt par lettre, avec ses numéros de ligne et de
 * bloc (fournis par mega-core dans l'ordre de lecture, correctif 0.12.2).
 */
function word(text: string, x: number, y: number, line: number, block = 0): TextBox[] {
  return [...text].map((ch, k) => ({ ch, rect: { x: x + 10 * k, y, w: 10, h: 12 }, line, block }));
}

// « Pa » sur une ligne (y = 100), « ge » sur la suivante (y = 80), même paragraphe.
const boxes: TextBox[] = [...word('Pa', 10, 100, 0), ...word('ge', 10, 80, 1)];

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

describe('charIndexAt follows blocks and lines', () => {
  // Une ligne courte (« Pa ») au-dessus d'une ligne longue qui va jusqu'à x = 200.
  const lines = [...word('Pa', 10, 100, 0), ...word('abcdefghijklmnopqrst', 10, 80, 1)];
  it('stays on the line when the pointer goes past its end', () => {
    expect(charIndexAt(lines, 190, 105)).toBe(1);
  });
  it('picks the vertically nearest line between two lines', () => {
    expect(lines[charIndexAt(lines, 15, 94)].rect.y).toBe(80);
    expect(lines[charIndexAt(lines, 15, 98)].rect.y).toBe(100);
  });
  it('picks the nearer column in a gutter', () => {
    const cols = [...word('ab', 10, 100, 0, 0), ...word('cd', 100, 100, 1, 1)];
    expect(cols[charIndexAt(cols, 45, 105)].ch).toBe('b');
    expect(cols[charIndexAt(cols, 85, 105)].ch).toBe('c');
  });
  it('stays in its column across a paragraph gap', () => {
    // Colonne gauche : deux paragraphes (y = 120, puis y = 80) ; colonne droite : une ligne
    // à y = 100, en face du blanc entre les deux paragraphes.
    const page = [
      ...word('gauche un', 10, 120, 0, 0),
      ...word('gauche deux', 10, 80, 1, 1),
      ...word('droite', 250, 100, 2, 2),
    ];
    expect(page[charIndexAt(page, 60, 104)].block).not.toBe(2);
    expect(page[charIndexAt(page, 2, 104)].block).not.toBe(2);
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
  it('keeps a superscript on its line', () => {
    const b: TextBox[] = [
      ...word('mc', 10, 100, 0),
      { ch: '²', rect: { x: 30, y: 105, w: 6, h: 8 }, line: 0, block: 0 },
    ];
    expect(selectionRects(b, 0, 2)).toHaveLength(1);
  });
});

describe('wordAt', () => {
  it('stops a word at the end of its line', () => {
    // « un mot » puis « suite » sur la ligne suivante, sans saut de ligne entre les deux.
    const b = [...word('un mot', 10, 100, 0), ...word('suite', 10, 80, 1)];
    expect(wordAt(b, 5)).toEqual([3, 5]);
    expect(wordAt(b, 6)).toEqual([6, 10]);
  });
  it('keeps a superscript in its word', () => {
    const b: TextBox[] = [
      ...word('mc', 10, 100, 0),
      { ch: '²', rect: { x: 30, y: 105, w: 6, h: 8 }, line: 0, block: 0 },
    ];
    expect(wordAt(b, 0)).toEqual([0, 2]);
  });
});

describe('selectedText', () => {
  it('joins characters and breaks lines', () => {
    expect(selectedText(boxes, 0, 3)).toBe('Pa\nge');
    expect(selectedText(boxes, 1, 2)).toBe('a\ng');
  });
  it('puts a space between a label and its value', () => {
    const b = [...word('Nom:', 10, 100, 0), ...word('Alice', 200, 100, 0)];
    expect(selectedText(b, 0, b.length - 1)).toBe('Nom: Alice');
  });
  it('puts a space between two words of a right-to-left line', () => {
    // Lot 13 : en hébreu, l'ordre de lecture va vers les x décroissants.
    const b: TextBox[] = [
      { ch: 'ב', rect: { x: 300, y: 100, w: 10, h: 12 }, line: 0, block: 0 },
      { ch: 'א', rect: { x: 290, y: 100, w: 10, h: 12 }, line: 0, block: 0 },
      { ch: 'ד', rect: { x: 200, y: 100, w: 10, h: 12 }, line: 0, block: 0 },
    ];
    expect(selectedText(b, 0, 2)).toBe('בא ד');
  });
  it('separates blocks with a blank line, like the text export', () => {
    // La fin d'une colonne puis le haut de la suivante : un changement de bloc.
    const b = [...word('fin', 10, 100, 0, 0), ...word('debut', 250, 300, 1, 1)];
    expect(selectedText(b, 0, b.length - 1)).toBe('fin\n\ndebut');
  });
  it('copies a superscript on its line', () => {
    const b: TextBox[] = [
      ...word('mc', 10, 100, 0),
      { ch: '²', rect: { x: 30, y: 105, w: 6, h: 8 }, line: 0, block: 0 },
    ];
    expect(selectedText(b, 0, 2)).toBe('mc²');
  });
});
