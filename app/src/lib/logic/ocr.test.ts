// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { ocrPages } from './ocr';

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
