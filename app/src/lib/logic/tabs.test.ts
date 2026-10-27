// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { tabKey } from './tabs';

describe('tabKey (C28)', () => {
  it('arrows move between tabs, wrapping; Home and End jump to the ends', () => {
    expect(tabKey('ArrowRight', 0, 3)).toBe(1);
    expect(tabKey('ArrowRight', 2, 3)).toBe(0);
    expect(tabKey('ArrowLeft', 0, 3)).toBe(2);
    expect(tabKey('Home', 2, 3)).toBe(0);
    expect(tabKey('End', 0, 3)).toBe(2);
  });
  it('other keys do nothing', () => {
    expect(tabKey('Enter', 1, 3)).toBeNull();
    expect(tabKey('ArrowDown', 1, 3)).toBeNull();
  });
});
