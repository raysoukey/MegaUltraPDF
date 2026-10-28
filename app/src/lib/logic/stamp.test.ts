// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { stampRect } from './stamp';

describe('stampRect', () => {
  it('places a 220 × 70 stamp 36 pt from the chosen corner', () => {
    expect(stampRect({ width_pt: 595, height_pt: 842 }, 'bottom_right')).toEqual({
      x: 595 - 36 - 220,
      y: 36,
      w: 220,
      h: 70,
    });
    expect(stampRect({ width_pt: 595, height_pt: 842 }, 'top_left')).toEqual({
      x: 36,
      y: 842 - 36 - 70,
      w: 220,
      h: 70,
    });
  });
  it('shrinks the stamp on a small page and never leaves it', () => {
    const r = stampRect({ width_pt: 200, height_pt: 100 }, 'bottom_left');
    expect(r.x).toBeGreaterThanOrEqual(0);
    expect(r.y).toBeGreaterThanOrEqual(0);
    expect(r.x + r.w).toBeLessThanOrEqual(200);
    expect(r.y + r.h).toBeLessThanOrEqual(100);
    expect(r.w).toBeGreaterThanOrEqual(10);
    expect(r.h).toBeGreaterThanOrEqual(10);
  });
});
