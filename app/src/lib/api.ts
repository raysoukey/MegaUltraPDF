// SPDX-License-Identifier: GPL-3.0-or-later
// Enveloppes typées des commandes Tauri. Les types reflètent ceux définis en Rust
// (`app/src-tauri/src/commands.rs` et `error.rs`) : si l'un change, changer l'autre.

import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';

/** Dimensions d'une page en points (miroir de `mega_core::PageSize`). */
export interface PageSize {
  width_pt: number;
  height_pt: number;
}

/** Description d'un document ouvert (miroir de `DocumentInfo`). */
export interface DocumentInfo {
  id: number;
  path: string;
  title: string | null;
  page_count: number;
  pages: PageSize[];
}

/** Erreur renvoyée par le backend (miroir de `AppError`). */
export interface AppError {
  kind: 'password_required' | 'invalid_pdf' | 'not_found' | 'io' | 'internal';
  message: string;
}

/** Vrai si `e` a la forme d'une `AppError` sérialisée. */
export function isAppError(e: unknown): e is AppError {
  return typeof e === 'object' && e !== null && 'kind' in e && 'message' in e;
}

export async function openDocument(path: string, password?: string): Promise<DocumentInfo> {
  return invoke<DocumentInfo>('open_document', { path, password: password ?? null });
}

export async function closeDocument(id: number): Promise<void> {
  await invoke('close_document', { id });
}

/** Fichier PDF passé en argument au lancement de l'application, s'il y en a un. */
export async function startupFile(): Promise<string | null> {
  return invoke<string | null>('startup_file');
}

/** Ouvre le dialogue natif et retourne le chemin choisi, ou `null` si annulé. */
export async function pickPdfFile(): Promise<string | null> {
  const picked = await open({
    multiple: false,
    directory: false,
    filters: [{ name: 'PDF', extensions: ['pdf'] }],
  });
  return typeof picked === 'string' ? picked : null;
}
