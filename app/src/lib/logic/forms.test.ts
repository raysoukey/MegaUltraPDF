// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import type { FormField } from '$lib/api';
import { canvasStrokesToPdf, fieldInputType, fitRect, missingRequired } from './forms';

function field(partial: Partial<FormField>): FormField {
  return {
    page: 0,
    index: 0,
    name: 'f',
    kind: 'text',
    value: '',
    checked: null,
    options: [],
    rect: { x: 0, y: 0, w: 10, h: 10 },
    read_only: false,
    required: false,
    multiline: false,
    password: false,
    ...partial,
  };
}

describe('missingRequired', () => {
  it('lists required text fields that are empty and required checkboxes that are unchecked', () => {
    const fields = [
      field({ name: 'a', required: true, value: '' }),
      field({ name: 'b', required: true, value: 'x' }),
      field({ name: 'c', required: true, kind: 'checkbox', value: null, checked: false }),
      field({ name: 'd', required: false, value: '' }),
      field({ name: 'e', required: true, value: '   ' }),
    ];
    expect(missingRequired(fields).map((f) => f.name)).toEqual(['a', 'c', 'e']);
  });
  it('counts a required radio group once, and not at all when one button is checked', () => {
    const group = [
      field({ name: 'g', kind: 'radio', required: true, value: null, checked: true, index: 1 }),
      field({ name: 'g', kind: 'radio', required: true, value: null, checked: false, index: 2 }),
    ];
    expect(missingRequired(group)).toEqual([]);
    const empty = group.map((f) => ({ ...f, checked: false }));
    expect(missingRequired(empty).map((f) => f.index)).toEqual([1]);
  });
});

describe('fieldInputType', () => {
  it('maps kinds and flags to HTML controls', () => {
    expect(fieldInputType(field({ kind: 'text' }))).toBe('text');
    expect(fieldInputType(field({ kind: 'text', multiline: true }))).toBe('textarea');
    expect(fieldInputType(field({ kind: 'text', password: true }))).toBe('password');
    expect(fieldInputType(field({ kind: 'checkbox' }))).toBe('checkbox');
    expect(fieldInputType(field({ kind: 'radio' }))).toBe('radio');
    expect(fieldInputType(field({ kind: 'combo' }))).toBe('select');
    expect(fieldInputType(field({ kind: 'list' }))).toBe('select');
    expect(fieldInputType(field({ kind: 'button' }))).toBe('none');
  });
});

describe('fitRect', () => {
  it('keeps the aspect ratio and centres the image (same rule as Rust)', () => {
    expect(fitRect(20, 10, { x: 50, y: 400, w: 100, h: 100 })).toEqual({
      x: 50,
      y: 425,
      w: 100,
      h: 50,
    });
    expect(fitRect(10, 20, { x: 0, y: 0, w: 100, h: 100 })).toEqual({ x: 25, y: 0, w: 50, h: 100 });
  });
});

describe('canvasStrokesToPdf', () => {
  it('maps canvas pixels (origin top-left) into the PDF rect, keeping proportions', () => {
    // Canvas 400×200 dans un rectangle PDF 200×100 : échelle 0,5 ; y inversé.
    const strokes = canvasStrokesToPdf(
      [
        [
          { x: 0, y: 0 },
          { x: 400, y: 200 },
        ],
      ],
      { w: 400, h: 200 },
      { x: 100, y: 500, w: 200, h: 100 },
    );
    expect(strokes).toEqual([
      [
        [100, 600],
        [300, 500],
      ],
    ]);
  });
  it('centres when the ratios differ and drops strokes with fewer than two points', () => {
    const strokes = canvasStrokesToPdf(
      [
        [{ x: 0, y: 0 }],
        [
          { x: 0, y: 100 },
          { x: 400, y: 100 },
        ],
      ],
      { w: 400, h: 200 },
      { x: 0, y: 0, w: 100, h: 100 },
    );
    // Échelle 0,25 → dessin 100×50 centré verticalement (y de 25 à 75).
    expect(strokes).toEqual([
      [
        [0, 50],
        [100, 50],
      ],
    ]);
  });
});
