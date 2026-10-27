// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { menuKey } from './menu';

const all = [true, true, true];

describe('menuKey', () => {
  it('moves with the arrows, wrapping around', () => {
    expect(menuKey('ArrowDown', 0, all)).toEqual({ index: 1 });
    expect(menuKey('ArrowDown', 2, all)).toEqual({ index: 0 });
    expect(menuKey('ArrowUp', 0, all)).toEqual({ index: 2 });
    expect(menuKey('ArrowDown', -1, all)).toEqual({ index: 0 });
    expect(menuKey('ArrowUp', -1, all)).toEqual({ index: 2 });
  });
  it('jumps to the ends with Home and End', () => {
    expect(menuKey('Home', 2, all)).toEqual({ index: 0 });
    expect(menuKey('End', 0, all)).toEqual({ index: 2 });
  });
  it('skips disabled items', () => {
    const some = [true, false, true];
    expect(menuKey('ArrowDown', 0, some)).toEqual({ index: 2 });
    expect(menuKey('ArrowUp', 2, some)).toEqual({ index: 0 });
    expect(menuKey('Home', 2, [false, true, true])).toEqual({ index: 1 });
    expect(menuKey('End', 0, [true, true, false])).toEqual({ index: 1 });
    expect(menuKey('ArrowDown', 0, [false, false])).toBeNull();
  });
  it('closes on Escape and Tab, activates on Enter and Space, ignores the rest', () => {
    expect(menuKey('Escape', 1, all)).toEqual({ close: true });
    expect(menuKey('Tab', 1, all)).toEqual({ close: true });
    expect(menuKey('Enter', 1, all)).toEqual({ activate: true });
    expect(menuKey(' ', 1, all)).toEqual({ activate: true });
    expect(menuKey('a', 1, all)).toBeNull();
  });
});
