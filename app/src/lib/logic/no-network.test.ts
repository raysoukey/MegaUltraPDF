// SPDX-License-Identifier: GPL-3.0-or-later
// Garde-fou « aucun appel réseau à l'exécution » (CLAUDE.md, FONCTIONNALITES) : l'interface
// n'utilise aucune API réseau, la CSP n'autorise que l'IPC de Tauri, et le code Rust du
// projet n'emploie aucun client réseau. (Tauri lie lui-même `reqwest` ; c'est notre code et
// la CSP qui garantissent qu'il ne sert pas.)
import { describe, expect, it } from 'vitest';
// @ts-expect-error type error without @types/node package (comme vite.config.js)
import { readdirSync, readFileSync } from 'node:fs';

const ROOT = new URL('../../../../', import.meta.url);

/** Fichiers (chemins relatifs à la racine du dépôt) d'un dossier, récursivement. */
function files(dir: string, extensions: string[]): string[] {
  const entries: string[] = readdirSync(new URL(dir, ROOT), { recursive: true, encoding: 'utf8' });
  return entries
    .map((e) => `${dir}/${e.replaceAll('\\', '/')}`)
    .filter((p) => extensions.some((ext) => p.endsWith(ext)));
}

const read = (path: string): string => readFileSync(new URL(path, ROOT), 'utf8');

describe('no network at runtime', () => {
  it('the interface code calls no network API', () => {
    const banned = /\bfetch\s*\(|XMLHttpRequest|new\s+WebSocket|sendBeacon|new\s+EventSource/;
    const sources = files('app/src', ['.ts', '.svelte']).filter((p) => !p.endsWith('.test.ts'));
    expect(sources.length).toBeGreaterThan(50);
    expect(sources.filter((p) => banned.test(read(p)))).toEqual([]);
  });

  it('the content security policy only lets the page talk to Tauri', () => {
    const conf = JSON.parse(read('app/src-tauri/tauri.conf.json'));
    const csp = conf.app.security.csp;
    expect(csp['default-src']).toBe("'self'");
    expect(csp['connect-src'].split(/\s+/)).toEqual(['ipc:', 'http://ipc.localhost']);
  });

  it('no capability grants the HTTP plugin', () => {
    for (const path of files('app/src-tauri/capabilities', ['.json'])) {
      const permissions: unknown[] = JSON.parse(read(path)).permissions;
      expect(permissions.filter((p) => String(p).startsWith('http:'))).toEqual([]);
    }
  });

  it('the Rust code of the project uses no network client', () => {
    const banned = /\b(reqwest|ureq|hyper)::|TcpStream|UdpSocket/;
    const sources = [...files('crates', ['.rs']), ...files('app/src-tauri/src', ['.rs'])];
    expect(sources.length).toBeGreaterThan(20);
    expect(sources.filter((p) => banned.test(read(p)))).toEqual([]);
  });
});
