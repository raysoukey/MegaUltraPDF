// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { clickSelection, moveTarget, reorder, shiftAfterInsert, slotAtY } from './pageselection';

describe('clickSelection', () => {
  it('plain click selects only the clicked page and sets the anchor', () => {
    expect(clickSelection([1, 2], 1, 4, { ctrl: false, shift: false })).toEqual({
      selected: [4],
      anchor: 4,
    });
  });
  it('ctrl toggles the page without touching the others', () => {
    expect(clickSelection([1, 2], 1, 4, { ctrl: true, shift: false })).toEqual({
      selected: [1, 2, 4],
      anchor: 4,
    });
    expect(clickSelection([1, 2, 4], 4, 2, { ctrl: true, shift: false })).toEqual({
      selected: [1, 4],
      anchor: 2,
    });
  });
  it('shift selects the range from the anchor, in either direction, keeping the anchor', () => {
    expect(clickSelection([1], 1, 4, { ctrl: false, shift: true })).toEqual({
      selected: [1, 2, 3, 4],
      anchor: 1,
    });
    expect(clickSelection([5], 5, 3, { ctrl: false, shift: true })).toEqual({
      selected: [3, 4, 5],
      anchor: 5,
    });
  });
  it('shift without an anchor behaves like a plain click', () => {
    expect(clickSelection([], null, 2, { ctrl: false, shift: true })).toEqual({
      selected: [2],
      anchor: 2,
    });
  });
});

describe('reorder / moveTarget', () => {
  // 5 pages : 0 1 2 3 4 ; `slot` = insérer avant la page d'index `slot` (5 = à la fin).
  it('moves a single page forward and backward', () => {
    expect(reorder(5, [1], 4)).toEqual([0, 2, 3, 1, 4]);
    expect(reorder(5, [3], 1)).toEqual([0, 3, 1, 2, 4]);
    expect(reorder(5, [0], 5)).toEqual([1, 2, 3, 4, 0]);
  });
  it('moves a non-contiguous selection as a block, in page order', () => {
    expect(reorder(5, [3, 1], 5)).toEqual([0, 2, 4, 1, 3]);
    expect(reorder(5, [1, 3], 0)).toEqual([1, 3, 0, 2, 4]);
  });
  it('moveTarget is the insertion index after removal, or null when nothing would move', () => {
    expect(moveTarget(5, [1], 4)).toBe(3);
    expect(moveTarget(5, [3], 1)).toBe(1);
    expect(moveTarget(5, [1], 1)).toBeNull();
    expect(moveTarget(5, [1], 2)).toBeNull();
    expect(moveTarget(5, [1, 2], 1)).toBeNull();
    expect(moveTarget(5, [1, 2], 3)).toBeNull();
    expect(moveTarget(5, [], 3)).toBeNull();
  });
});

describe('shiftAfterInsert', () => {
  it('shifts the selected indices at or after the insertion point', () => {
    expect(shiftAfterInsert([1, 3], 1, 1)).toEqual([2, 4]);
    expect(shiftAfterInsert([0, 4], 2, 3)).toEqual([0, 7]);
    expect(shiftAfterInsert([], 0, 1)).toEqual([]);
  });
});

describe('slotAtY (A22)', () => {
  // Trois vignettes de 100 px, espacées de 20 px.
  const tops = [0, 120, 240];
  const heights = [100, 100, 100];
  it('a drop between two thumbnails goes between them, not at the end', () => {
    expect(slotAtY(110, tops, heights)).toBe(1);
    expect(slotAtY(230, tops, heights)).toBe(2);
  });
  it('upper half before the page, lower half after', () => {
    expect(slotAtY(30, tops, heights)).toBe(0);
    expect(slotAtY(80, tops, heights)).toBe(1);
    expect(slotAtY(400, tops, heights)).toBe(3);
  });
});
