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

describe('localised sizes (C26)', () => {
  it('uses the decimal separator of each language', async () => {
    const { fr, en } = await import('$lib/strings');
    expect(fr.bytes(1536)).toBe('1,5 Ko');
    expect(en.bytes(1536)).toBe('1.5 KB');
    expect(fr.bytes(2 * 1024 * 1024)).toBe('2,0 Mo');
  });
});

describe('strings in use (C29)', () => {
  it('every top-level key of the dictionaries is used somewhere in the interface', async () => {
    // @ts-expect-error type error without @types/node package (comme vite.config.js)
    const { readdirSync, readFileSync } = await import('node:fs');
    const { fr } = await import('$lib/strings');
    const root = new URL('../../', import.meta.url);
    const sources = (readdirSync(root, { recursive: true, encoding: 'utf8' }) as string[])
      .filter((f) => (f.endsWith('.ts') || f.endsWith('.svelte')) && !f.endsWith('.test.ts'))
      .filter((f) => !f.replaceAll('\\', '/').endsWith('lib/strings.ts'))
      .map((f) => readFileSync(new URL(f, root), 'utf8') as string)
      .join('\n');
    const unused = Object.keys(fr).filter((key) => !new RegExp(`\\.${key}\\b`).test(sources));
    expect(unused).toEqual([]);
  });
});
