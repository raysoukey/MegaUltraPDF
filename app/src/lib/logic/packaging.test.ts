// SPDX-License-Identifier: GPL-3.0-or-later
// Garde-fou des réglages de paquets (Lot 13, audit P2 et P7) : ils ne se voient qu'à
// l'installation, sur une autre machine ; un test les empêche de disparaître en silence.
import { describe, expect, it } from 'vitest';
// @ts-expect-error type error without @types/node package (comme vite.config.js)
import { readFileSync } from 'node:fs';

const ROOT = new URL('../../../../', import.meta.url);
const read = (path: string): string => readFileSync(new URL(path, ROOT), 'utf8');

describe('packaging settings', () => {
  const bundle = JSON.parse(read('app/src-tauri/tauri.conf.json')).bundle;

  it('ships the WebView2 bootstrapper inside the Windows installers', () => {
    expect(bundle.windows.webviewInstallMode).toEqual({ type: 'embedBootstrapper' });
  });
  it('declares the oldest macOS version Tauri 2 supports', () => {
    expect(bundle.macOS.minimumSystemVersion).toBe('10.15');
  });
  it('recommends Tesseract in the Debian package (optional OCR)', () => {
    expect(bundle.linux.deb.recommends).toContain('tesseract-ocr');
  });
  it('builds a universal macOS package (Intel and Apple Silicon)', () => {
    const release = read('.github/workflows/release.yml');
    expect(release).toContain('universal-apple-darwin');
    expect(release).toContain('lipo -archs');
  });
});
