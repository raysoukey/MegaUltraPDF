// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { MAX_SCALE, MIN_SCALE, clampScale, fitPageScale, fitWidthScale, nextZoom } from './zoom';

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
