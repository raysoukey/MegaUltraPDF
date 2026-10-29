// SPDX-License-Identifier: GPL-3.0-or-later
// Les icônes d'actions (Lot 12) : chaque nom a son image, produite par `cargo xtask brand`.
import { describe, expect, it } from 'vitest';
// @ts-expect-error type error without @types/node package (comme vite.config.js)
import { existsSync, readFileSync } from 'node:fs';
import { ICON_NAMES, iconSrc } from './icons';

const ROOT = new URL('../../../../', import.meta.url);

/** Largeur et hauteur d'un PNG, lues dans son en-tête (signature, puis bloc IHDR). */
function pngSize(bytes: Uint8Array): [number, number] | null {
  const signature = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
  if (!signature.every((b, i) => bytes[i] === b)) return null;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return [view.getUint32(16), view.getUint32(20)];
}

describe('action icons', () => {
  it('lists the same icons, in the same order, as cargo xtask brand', () => {
    const rust: string = readFileSync(new URL('xtask/src/brand.rs', ROOT), 'utf8');
    const block = rust.slice(rust.indexOf('ICON_NAMES'), rust.indexOf('];'));
    const names = [...block.matchAll(/"([a-z]+)"/g)].map((m) => m[1]);
    expect(names.length).toBe(12);
    expect([...ICON_NAMES]).toEqual(names);
  });

  it('serves each icon from the static folder', () => {
    expect(iconSrc('open')).toBe('/icons/open.png');
  });

  it('has a 64 × 64 PNG for every icon', () => {
    for (const name of ICON_NAMES) {
      const url = new URL(`app/static${iconSrc(name)}`, ROOT);
      expect(existsSync(url), name).toBe(true);
      expect(pngSize(readFileSync(url)), name).toEqual([64, 64]);
    }
  });
});
