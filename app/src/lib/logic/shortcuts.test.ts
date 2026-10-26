// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { SHORTCUT_HELP, actionFor } from './shortcuts';

const key = (k: string, mods: Partial<{ ctrl: boolean; shift: boolean; alt: boolean }> = {}) => ({
  key: k,
  ctrl: false,
  shift: false,
  alt: false,
  ...mods,
});

describe('actionFor', () => {
  it('maps the main shortcuts', () => {
    expect(actionFor(key('o', { ctrl: true }))).toBe('open');
    expect(actionFor(key('w', { ctrl: true }))).toBe('close');
    expect(actionFor(key('f', { ctrl: true }))).toBe('find');
    expect(actionFor(key('F3'))).toBe('findNext');
    expect(actionFor(key('F3', { shift: true }))).toBe('findPrev');
    expect(actionFor(key('+', { ctrl: true }))).toBe('zoomIn');
    expect(actionFor(key('=', { ctrl: true }))).toBe('zoomIn');
    expect(actionFor(key('-', { ctrl: true }))).toBe('zoomOut');
    expect(actionFor(key('0', { ctrl: true }))).toBe('zoomReset');
    expect(actionFor(key('1', { ctrl: true }))).toBe('fitWidth');
    expect(actionFor(key('2', { ctrl: true }))).toBe('fitPage');
    expect(actionFor(key('r'))).toBe('rotate');
    expect(actionFor(key('Home'))).toBe('firstPage');
    expect(actionFor(key('End'))).toBe('lastPage');
    expect(actionFor(key('PageDown'))).toBe('nextPage');
    expect(actionFor(key('PageUp'))).toBe('prevPage');
    expect(actionFor(key('F1'))).toBe('help');
    expect(actionFor(key('Escape'))).toBe('escape');
    expect(actionFor(key('i', { ctrl: true }))).toBe('properties');
  });
  it('ignores unknown or wrongly modified keys', () => {
    expect(actionFor(key('o'))).toBeNull();
    expect(actionFor(key('r', { ctrl: true }))).toBeNull();
    expect(actionFor(key('x', { ctrl: true }))).toBeNull();
  });
});

describe('SHORTCUT_HELP', () => {
  it('describes every action once, with a label', () => {
    const actions = SHORTCUT_HELP.map((h) => h.action);
    expect(new Set(actions).size).toBe(actions.length);
    expect(SHORTCUT_HELP.find((h) => h.action === 'open')?.keys).toContain('Ctrl+O');
    expect(SHORTCUT_HELP.every((h) => h.label.length > 0)).toBe(true);
  });
});
