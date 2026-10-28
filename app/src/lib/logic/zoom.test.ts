// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import {
  MAX_SCALE,
  MIN_SCALE,
  anchoredScroll,
  clampScale,
  fitPageScale,
  fitWidthScale,
  nextZoom,
} from './zoom';

const a4 = { width_pt: 595, height_pt: 842 };

describe('nextZoom', () => {
  it('moves to the next step strictly above or below', () => {
    expect(nextZoom(1, 1)).toBe(1.1);
    expect(nextZoom(1.05, 1)).toBe(1.1);
    expect(nextZoom(1.05, -1)).toBe(1);
    expect(nextZoom(1, -1)).toBe(0.9);
  });
  it('stays within bounds', () => {
    expect(nextZoom(MAX_SCALE, 1)).toBe(MAX_SCALE);
    expect(nextZoom(MIN_SCALE, -1)).toBe(MIN_SCALE);
  });
});

describe('fit scales', () => {
  it('fits the width of the container', () => {
    expect(fitWidthScale(a4, 800, 0)).toBeCloseTo(800 / 595, 4);
    expect(fitWidthScale(a4, 800, 16)).toBeCloseTo(768 / 595, 4);
  });
  it('fits the whole page in the container', () => {
    expect(fitPageScale(a4, 800, 600, 0)).toBeCloseTo(600 / 842, 4);
    expect(fitPageScale(a4, 400, 2000, 0)).toBeCloseTo(400 / 595, 4);
  });
  it('clamps absurd values', () => {
    expect(clampScale(100)).toBe(MAX_SCALE);
    expect(clampScale(0)).toBe(MIN_SCALE);
    expect(clampScale(Number.NaN)).toBe(1);
  });
});

describe('anchoredScroll (A14)', () => {
  it('keeps the point under the cursor in place', () => {
    // Point de contenu sous le curseur : 100 + 50 = 150 ; à l'échelle × 2 : 300.
    expect(anchoredScroll(100, 50, 2, 0)).toBe(250);
  });
  it('does not scale the fixed part before the point (gaps between pages)', () => {
    // 32 px d'écarts au-dessus du point ne grandissent pas.
    expect(anchoredScroll(100, 50, 2, 32)).toBe((150 - 32) * 2 + 32 - 50);
  });
  it('never scrolls before the start', () => {
    expect(anchoredScroll(0, 10, 0.5, 0)).toBe(0);
  });
});
