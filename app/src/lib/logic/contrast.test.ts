// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
// Vitest vide le contenu des fichiers CSS importés (même `?raw`) : on lit le fichier.
// @ts-expect-error type error without @types/node package (comme vite.config.js)
import { readFileSync } from 'node:fs';
import { contrastRatio, themeTokens } from './contrast';

const css: string = readFileSync(new URL('../../app.css', import.meta.url), 'utf8');

describe('contrastRatio', () => {
  it('gives the WCAG reference values', () => {
    expect(contrastRatio('#000000', '#ffffff')).toBeCloseTo(21, 1);
    expect(contrastRatio('#ffffff', '#ffffff')).toBeCloseTo(1, 5);
    expect(contrastRatio('#777777', '#ffffff')).toBeCloseTo(4.48, 1);
  });
});

describe('theme tokens', () => {
  for (const theme of ['light', 'dark'] as const) {
    it(`${theme}: body and muted text meet 4.5:1 on panels and background`, () => {
      const tk = themeTokens(css, theme);
      for (const surface of ['--bg', '--panel'] as const) {
        expect(contrastRatio(tk['--text'], tk[surface])).toBeGreaterThanOrEqual(7);
        expect(contrastRatio(tk['--muted'], tk[surface])).toBeGreaterThanOrEqual(4.5);
      }
      expect(contrastRatio(tk['--danger-text'], tk['--danger-bg'])).toBeGreaterThanOrEqual(4.5);
      // Contour de focus et éléments d'interface : 3:1 (WCAG 1.4.11).
      expect(contrastRatio(tk['--accent'], tk['--panel'])).toBeGreaterThanOrEqual(3);
    });
  }
});
