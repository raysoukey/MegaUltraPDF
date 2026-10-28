// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { boundPositions, initialPosition } from './position';

const base = {
  scale: 1.25,
  // Le dernier zoom utilisé (« custom ») ne doit pas remplacer le zoom choisi pour l'ouverture.
  zoomMode: 'custom' as const,
  defaultZoomMode: 'fitWidth' as const,
  restorePositions: true,
  positions: { 'C:/a.pdf': { page: 7, scale: 2, zoomMode: 'custom' as const } },
};

describe('initialPosition', () => {
  it('reopens a known file where it was left', () => {
    expect(initialPosition(base, 'C:/a.pdf')).toEqual({ page: 7, scale: 2, zoomMode: 'custom' });
  });
  it('uses the default zoom and the first page for an unknown file', () => {
    expect(initialPosition(base, 'C:/b.pdf')).toEqual({
      page: 0,
      scale: 1.25,
      zoomMode: 'fitWidth',
    });
    expect(initialPosition(base, null)).toEqual({ page: 0, scale: 1.25, zoomMode: 'fitWidth' });
  });
  it('uses the last scale when the preferred opening zoom is "last zoom used"', () => {
    expect(initialPosition({ ...base, defaultZoomMode: 'custom' }, 'C:/b.pdf')).toEqual({
      page: 0,
      scale: 1.25,
      zoomMode: 'custom',
    });
  });
  it('ignores the remembered position when restorePositions is off', () => {
    expect(initialPosition({ ...base, restorePositions: false }, 'C:/a.pdf')).toEqual({
      page: 0,
      scale: 1.25,
      zoomMode: 'fitWidth',
    });
  });
});

describe('boundPositions (A20)', () => {
  it('keeps the most recently remembered positions only', () => {
    const positions: Record<string, { page: number; scale: number; zoomMode: 'custom' }> = {};
    for (let i = 0; i < 120; i++)
      positions[`f${i}.pdf`] = { page: i, scale: 1, zoomMode: 'custom' };
    const kept = boundPositions(positions, 100);
    expect(Object.keys(kept)).toHaveLength(100);
    expect(kept['f0.pdf']).toBeUndefined();
    expect(kept['f119.pdf']?.page).toBe(119);
  });
});
