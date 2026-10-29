// SPDX-License-Identifier: GPL-3.0-or-later
// Fermeture de la fenêtre : quand l'interface intercepte la demande de fermeture
// (`onCloseRequested`, pour confirmer les modifications et enregistrer les préférences), Tauri
// ne ferme plus la fenêtre lui-même : c'est l'interface qui appelle `window.destroy()`. Sans
// la permission `core:window:allow-destroy`, cet appel est refusé et la fenêtre ne se ferme
// jamais (bogue réel : il fallait passer par le Gestionnaire des tâches).
import { describe, expect, it } from 'vitest';
// @ts-expect-error type error without @types/node package (comme vite.config.js)
import { readFileSync } from 'node:fs';

const ROOT = new URL('../../../../', import.meta.url);
const read = (path: string): string => readFileSync(new URL(path, ROOT), 'utf8');

describe('window close', () => {
  it('lets the interface destroy the window it holds open on close', () => {
    const page = read('app/src/routes/+page.svelte');
    const capabilities = JSON.parse(read('app/src-tauri/capabilities/default.json'));
    expect(page).toContain('onCloseRequested');
    expect(capabilities.permissions).toContain('core:window:allow-destroy');
  });
});
