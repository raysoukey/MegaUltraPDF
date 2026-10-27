// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { ocrOutcome, ocrPages } from './ocr';

describe('ocrPages', () => {
  it('keeps every page without text', () => {
    expect(ocrPages('untexted', 0, [true, false, false])).toEqual([1, 2]);
    expect(ocrPages('untexted', 0, [true, true])).toEqual([]);
  });
  it('keeps the current page only if it has no text', () => {
    expect(ocrPages('current', 2, [true, false, false])).toEqual([2]);
    expect(ocrPages('current', 0, [true, false, false])).toEqual([]);
  });
  it('ignores a current page outside the document', () => {
    expect(ocrPages('current', 5, [false])).toEqual([]);
  });
});

describe('ocrOutcome', () => {
  it('counts processed pages that gained text and those where nothing was recognized', () => {
    expect(ocrOutcome([0, 2, 3], [false, true, true, false])).toEqual({ recognized: 1, empty: 2 });
  });
  it('a missing page list counts nothing as recognized', () => {
    expect(ocrOutcome([1], [])).toEqual({ recognized: 0, empty: 1 });
  });
});
