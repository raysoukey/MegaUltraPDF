// SPDX-License-Identifier: GPL-3.0-or-later
// Contraste des couleurs (WCAG 2.x) et lecture des jetons de thème de `app.css`.

/** Luminance relative d'une couleur `#rrggbb` (WCAG 2.x, sRGB linéarisé). */
function luminance(hex: string): number {
  const v = hex.replace('#', '');
  const channel = (i: number) => {
    const c = Number.parseInt(v.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4);
}

/** Rapport de contraste entre deux couleurs `#rrggbb` (de 1 à 21). */
export function contrastRatio(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

/**
 * Les jetons `--nom: #rrggbb;` d'un thème : le bloc `:root { … }` pour le clair, le bloc
 * `:root[data-theme='dark'] { … }` pour le sombre.
 */
export function themeTokens(css: string, theme: 'light' | 'dark'): Record<string, string> {
  const marker = theme === 'light' ? ':root {' : ":root[data-theme='dark'] {";
  const start = css.indexOf(marker);
  const block = start < 0 ? '' : css.slice(start, css.indexOf('}', start));
  const tokens: Record<string, string> = {};
  for (const m of block.matchAll(/(--[a-z-]+):\s*(#[0-9a-fA-F]{6})\s*;/g)) tokens[m[1]] = m[2];
  return tokens;
}
