// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { nextFocus, radioTabStops } from './focus';

describe('nextFocus', () => {
  it('cycles forward with Tab and backward with Shift+Tab', () => {
    const usable = [true, true, true];
    expect(nextFocus(0, usable, false)).toBe(1);
    expect(nextFocus(2, usable, false)).toBe(0);
    expect(nextFocus(0, usable, true)).toBe(2);
    expect(nextFocus(1, usable, true)).toBe(0);
  });
  it('skips disabled or hidden elements', () => {
    expect(nextFocus(0, [true, false, true], false)).toBe(2);
    expect(nextFocus(2, [true, true, false], false)).toBe(0);
    expect(nextFocus(0, [true, false, false, true], true)).toBe(3);
  });
  it('enters the dialog when the focus is outside it', () => {
    expect(nextFocus(-1, [false, true, true], false)).toBe(1);
    expect(nextFocus(-1, [true, true, false], true)).toBe(1);
  });
  it('returns -1 when nothing can take the focus', () => {
    expect(nextFocus(0, [false, false], false)).toBe(-1);
    expect(nextFocus(-1, [], false)).toBe(-1);
  });
});

describe('radioTabStops (C24)', () => {
  it('keeps one Tab stop per radio group: the checked one, else the first', () => {
    expect(
      radioTabStops([
        { name: 'scope', checked: false },
        { name: 'scope', checked: true },
        { name: 'scope', checked: false },
        { name: 'other', checked: false },
        { name: 'other', checked: false },
      ]),
    ).toEqual([false, true, false, true, false]);
  });
  it('a non-radio element (no name) is always a stop', () => {
    expect(radioTabStops([{ name: null, checked: false }])).toEqual([true]);
  });
});
