// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { pagesToEvict } from './pagecache';

describe('pagesToEvict', () => {
  it('keeps the pages nearest to the current one', () => {
    // Distances à 11 : 0→11, 1→10, 2→9, 3→8, 10→1, 11→0, 12→1, 50→39, 51→40.
    const cached = [0, 1, 2, 3, 10, 11, 12, 50, 51];
    expect(pagesToEvict(cached, 11, 5).sort((a, b) => a - b)).toEqual([0, 1, 50, 51]);
    expect(pagesToEvict(cached, 11, 7).sort((a, b) => a - b)).toEqual([50, 51]);
  });
  it('evicts nothing under the limit', () => {
    expect(pagesToEvict([1, 2, 3], 2, 24)).toEqual([]);
  });
});
