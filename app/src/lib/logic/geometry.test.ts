// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { pdfRectToCss, rotatedSize } from './geometry';

const a4 = { width_pt: 595, height_pt: 842 };
const rect = { x: 72, y: 700, w: 128, h: 20 };

describe('pdfRectToCss', () => {
  it('flips the y axis without rotation', () => {
    expect(pdfRectToCss(rect, a4, 1, 'none')).toEqual({
      left: 72,
      top: 122,
      width: 128,
      height: 20,
    });
  });
  it('applies the scale', () => {
    expect(pdfRectToCss(rect, a4, 2, 'none')).toEqual({
      left: 144,
      top: 244,
      width: 256,
      height: 40,
    });
  });
  it('rotates 90° clockwise', () => {
    expect(pdfRectToCss(rect, a4, 1, 'cw90')).toEqual({
      left: 700,
      top: 72,
      width: 20,
      height: 128,
    });
  });
  it('rotates 180°', () => {
    expect(pdfRectToCss(rect, a4, 1, 'cw180')).toEqual({
      left: 395,
      top: 700,
      width: 128,
      height: 20,
    });
  });
  it('rotates 270° clockwise', () => {
    expect(pdfRectToCss(rect, a4, 1, 'cw270')).toEqual({
      left: 122,
      top: 395,
      width: 20,
      height: 128,
    });
  });
});

describe('rotatedSize', () => {
  it('swaps axes for quarter turns only', () => {
    expect(rotatedSize(a4, 'none')).toEqual({ w: 595, h: 842 });
    expect(rotatedSize(a4, 'cw90')).toEqual({ w: 842, h: 595 });
    expect(rotatedSize(a4, 'cw180')).toEqual({ w: 595, h: 842 });
  });
});
