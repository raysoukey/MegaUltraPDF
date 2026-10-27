// SPDX-License-Identifier: GPL-3.0-or-later
import { afterEach, describe, expect, it } from 'vitest';
import { en, fr, setLanguage, t } from '$lib/strings';
import { resolveLanguage } from './i18n';
import { shortcutHelp } from './shortcuts';

describe('resolveLanguage', () => {
  it('follows the system language for any French variant, English otherwise', () => {
    expect(resolveLanguage('system', 'fr-CA')).toBe('fr');
    expect(resolveLanguage('system', 'fr')).toBe('fr');
    expect(resolveLanguage('system', 'en-US')).toBe('en');
    expect(resolveLanguage('system', 'de-DE')).toBe('en');
    expect(resolveLanguage('system', '')).toBe('en');
  });
  it('lets an explicit choice win over the system language', () => {
    expect(resolveLanguage('fr', 'en-US')).toBe('fr');
    expect(resolveLanguage('en', 'fr-FR')).toBe('en');
  });
});

/** Toutes les clés d'un dictionnaire, récursivement (`a.b.c`), avec l'arité des fonctions. */
function shape(value: unknown, prefix = ''): string[] {
  if (typeof value === 'function') return [`${prefix}()${value.length}`];
  if (value && typeof value === 'object') {
    return Object.keys(value)
      .sort()
      .flatMap((k) => shape((value as Record<string, unknown>)[k], prefix ? `${prefix}.${k}` : k));
  }
  return [prefix];
}

describe('dictionaries', () => {
  it('English has exactly the shape of French (keys and function arity)', () => {
    expect(shape(en)).toEqual(shape(fr));
  });
  it('English strings are not French copies (a few telltale keys)', () => {
    expect(en.openFile).not.toBe(fr.openFile);
    expect(en.save).not.toBe(fr.save);
    expect(en.pages(2)).toBe('2 pages');
    expect(en.bytes(2048)).toBe('2.0 KB');
  });
});

describe('setLanguage', () => {
  afterEach(() => setLanguage('fr'));
  it('switches the shared `t` object and the generated shortcut help', () => {
    setLanguage('en');
    expect(t.save).toBe(en.save);
    expect(shortcutHelp()[0].label).toBe(en.actions[shortcutHelp()[0].action]);
    setLanguage('fr');
    expect(t.save).toBe(fr.save);
    expect(shortcutHelp()[0].label).toBe(fr.actions[shortcutHelp()[0].action]);
  });
});
