// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { actionFor, shortcutHelp, shortcutKeyFor } from './shortcuts';
import { fr } from '$lib/strings';
// @ts-expect-error type error without @types/node package (comme vite.config.js)
import { readdirSync, readFileSync } from 'node:fs';

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
  it('maps the editing shortcuts (Lot 2)', () => {
    expect(actionFor(key('z', { ctrl: true }))).toBe('undo');
    expect(actionFor(key('y', { ctrl: true }))).toBe('redo');
    expect(actionFor(key('z', { ctrl: true, shift: true }))).toBe('redo');
    expect(actionFor(key('s', { ctrl: true }))).toBe('save');
    expect(actionFor(key('s', { ctrl: true, shift: true }))).toBe('saveAs');
    expect(actionFor(key('z'))).toBeNull();
    expect(actionFor(key('m', { ctrl: true }))).toBe('merge');
    expect(actionFor(key('n'))).toBe('toolNote');
    expect(actionFor(key('h'))).toBe('toolHighlight');
    expect(actionFor(key('u'))).toBe('toolUnderline');
    expect(actionFor(key('k'))).toBe('toolStrikeout');
    expect(actionFor(key('t'))).toBe('toolText');
    expect(actionFor(key('d'))).toBe('toolInk');
    expect(actionFor(key('Delete'))).toBe('deleteAnnotation');
    expect(actionFor(key('d', { ctrl: true, shift: true }))).toBe('split');
  });
  it('opens the preferences with Ctrl+, (Lot 7)', () => {
    expect(actionFor(key(',', { ctrl: true }))).toBe('preferences');
    expect(actionFor(key(','))).toBeNull();
  });
  it('ignores unknown or wrongly modified keys', () => {
    expect(actionFor(key('o'))).toBeNull();
    expect(actionFor(key('r', { ctrl: true }))).toBeNull();
    expect(actionFor(key('x', { ctrl: true }))).toBeNull();
  });
});

describe('shortcutHelp', () => {
  it('describes every action once, with a label', () => {
    const actions = shortcutHelp().map((h) => h.action);
    expect(new Set(actions).size).toBe(actions.length);
    expect(shortcutHelp().find((h) => h.action === 'open')?.keys).toContain('Ctrl+O');
    expect(shortcutHelp().every((h) => h.label.length > 0)).toBe(true);
  });
});

describe('shortcut labels (A13, B10)', () => {
  it('are built from the shortcut table and the translated key names', () => {
    expect(shortcutKeyFor('open')).toBe(`${fr.keyCtrl}+O`);
    expect(shortcutKeyFor('saveAs')).toBe(`${fr.keyCtrl}+${fr.keyShift}+S`);
    expect(shortcutKeyFor('toolNote')).toBe('N');
  });
  it('no component hard-codes a « Ctrl+ » label', () => {
    const root = new URL('../../../../', import.meta.url);
    const dirs = ['app/src/lib/components', 'app/src/routes'];
    const offenders = dirs.flatMap((dir) =>
      (readdirSync(new URL(dir, root), { recursive: true, encoding: 'utf8' }) as string[])
        .filter((f) => f.endsWith('.svelte'))
        .map((f) => `${dir}/${f.replaceAll('\\', '/')}`)
        .filter((p) =>
          (readFileSync(new URL(p, root), 'utf8') as string)
            .split('\n')
            // Les commentaires peuvent citer une touche ; seul le texte affiché compte.
            .filter((line) => !/^\s*(\/\/|\/\*|\*|<!--)/.test(line))
            .some((line) => /Ctrl\+/.test(line)),
        ),
    );
    expect(offenders).toEqual([]);
  });
});
