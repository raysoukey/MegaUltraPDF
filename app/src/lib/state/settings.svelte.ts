// SPDX-License-Identifier: GPL-3.0-or-later
// Préférences persistantes (thème, zoom, mode, récents, positions de lecture).
// Stockées dans `settings.json` via le plugin store de Tauri ; hors Tauri (tests),
// les accès sont silencieusement ignorés.

import { pushRecent } from '$lib/logic/recent';

export type Theme = 'system' | 'light' | 'dark';
export type ZoomMode = 'custom' | 'fitWidth' | 'fitPage';
export type ViewMode = 'continuous' | 'single';

/** Position de lecture mémorisée pour un fichier. */
export interface ReadingPosition {
  page: number;
  scale: number;
  zoomMode: ZoomMode;
}

export interface Settings {
  theme: Theme;
  zoomMode: ZoomMode;
  scale: number;
  viewMode: ViewMode;
  sidebarOpen: boolean;
  recent: string[];
  positions: Record<string, ReadingPosition>;
  /** Auteur inscrit dans les annotations créées. */
  author: string;
}

const DEFAULTS: Settings = {
  theme: 'system',
  zoomMode: 'fitWidth',
  scale: 1,
  viewMode: 'continuous',
  sidebarOpen: true,
  recent: [],
  positions: {},
  author: '',
};

const STORE_FILE = 'settings.json';
const KEY = 'settings';

/** L'état réactif des préférences (runes Svelte 5). */
export const settings = $state<Settings>({ ...DEFAULTS });

type Store = {
  get<T>(key: string): Promise<T | undefined>;
  set(key: string, value: unknown): Promise<void>;
  save(): Promise<void>;
};

let store: Store | null = null;
let saveTimer: ReturnType<typeof setTimeout> | null = null;

/** Charge les préférences au démarrage ; les clés inconnues sont ignorées. */
export async function loadSettings(): Promise<void> {
  try {
    const { load } = await import('@tauri-apps/plugin-store');
    store = (await load(STORE_FILE, { autoSave: false, defaults: {} })) as unknown as Store;
    const saved = await store.get<Partial<Settings>>(KEY);
    if (saved) {
      for (const key of Object.keys(DEFAULTS) as (keyof Settings)[]) {
        if (saved[key] !== undefined) {
          (settings as unknown as Record<string, unknown>)[key] = saved[key];
        }
      }
    }
  } catch {
    store = null; // hors Tauri (tests) ou plugin indisponible : valeurs par défaut
  }
}

/** Sauvegarde différée (regroupe les modifications rapprochées). */
export function saveSettings(): void {
  if (!store) return;
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(async () => {
    try {
      await store?.set(KEY, $state.snapshot(settings));
      await store?.save();
    } catch {
      // une sauvegarde ratée ne doit jamais gêner la lecture
    }
  }, 300);
}

/** Sauvegarde immédiate (fermeture de la fenêtre) : n'attend pas le délai. */
export async function flushSettings(): Promise<void> {
  if (!store) return;
  if (saveTimer) {
    clearTimeout(saveTimer);
    saveTimer = null;
  }
  try {
    await store.set(KEY, $state.snapshot(settings));
    await store.save();
  } catch {
    // rien à faire de plus à la fermeture
  }
}

export function rememberRecent(path: string): void {
  settings.recent = pushRecent(settings.recent, path);
  saveSettings();
}

export function forgetRecent(path: string): void {
  settings.recent = settings.recent.filter((p) => p !== path);
  saveSettings();
}

export function clearRecent(): void {
  settings.recent = [];
  saveSettings();
}

export function rememberPosition(path: string, position: ReadingPosition): void {
  settings.positions = { ...settings.positions, [path]: position };
  saveSettings();
}
