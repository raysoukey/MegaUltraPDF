// SPDX-License-Identifier: GPL-3.0-or-later
// Enveloppes typées des commandes Tauri. Les types reflètent ceux définis en Rust
// (`crates/mega-core` et `app/src-tauri/src/commands.rs`) : si l'un change, changer l'autre.

import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';

/** Dimensions d'une page en points (miroir de `mega_core::PageSize`). */
export interface PageSize {
  width_pt: number;
  height_pt: number;
}

/** Rectangle en points PDF, origine en bas à gauche (miroir de `mega_core::Rect`). */
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Rotation d'affichage (miroir de `mega_core::Rotation`, sérialisée en minuscules). */
export type Rotation = 'none' | 'cw90' | 'cw180' | 'cw270';

/** Priorité d'un rendu (miroir de `mega_core::service::Priority`). */
export type Priority = 'visible' | 'prefetch' | 'thumbnail';

/** Description d'un document ouvert (miroir de `DocumentSummary`). */
export interface DocumentSummary {
  id: number;
  path: string | null;
  title: string | null;
  page_count: number;
  pages: PageSize[];
  generation: number;
}

/** Un caractère positionné (miroir de `TextBox`). */
export interface TextBox {
  ch: string;
  rect: Rect;
}

/** Une occurrence de recherche (miroir de `SearchHit`). */
export interface SearchHit {
  page: number;
  rects: Rect[];
}

/** Un signet (miroir de `OutlineNode`). */
export interface OutlineNode {
  title: string;
  page: number | null;
  children: OutlineNode[];
}

/** Cible d'un lien (miroir de `LinkTarget`, sérialisé `{ kind, value }`). */
export type LinkTarget = { kind: 'page'; value: number } | { kind: 'uri'; value: string };

/** Un lien de page (miroir de `PageLink`). */
export interface PageLink {
  rect: Rect;
  target: LinkTarget;
}

/** Propriétés du document (miroir de `DocumentProperties`). */
export interface DocumentProperties {
  title: string | null;
  author: string | null;
  subject: string | null;
  keywords: string | null;
  creator: string | null;
  producer: string | null;
  creation_date: string | null;
  modification_date: string | null;
  pdf_version: string;
  page_count: number;
}

/** Erreur renvoyée par le backend (miroir de `AppError`). */
export interface AppError {
  kind: 'password_required' | 'wrong_password' | 'invalid_pdf' | 'not_found' | 'io' | 'internal';
  message: string;
}

/** Vrai si `e` a la forme d'une `AppError` sérialisée. */
export function isAppError(e: unknown): e is AppError {
  return typeof e === 'object' && e !== null && 'kind' in e && 'message' in e;
}

export async function openDocument(path: string, password?: string): Promise<DocumentSummary> {
  return invoke<DocumentSummary>('open_document', { path, password: password ?? null });
}

export async function closeDocument(id: number): Promise<void> {
  await invoke('close_document', { id });
}

/**
 * Déclare au backend la génération courante du document (incrémentée côté frontend à
 * chaque changement de zoom ou de rotation). Le backend ne recule jamais ; il annule les
 * rendus demandés avec une génération plus ancienne.
 */
export async function setGeneration(id: number, generation: number): Promise<number> {
  return invoke<number>('set_generation', { id, generation });
}

export async function textLayer(id: number, page: number): Promise<TextBox[]> {
  return invoke<TextBox[]>('text_layer', { id, page });
}

export async function searchDocument(
  id: number,
  query: string,
  matchCase = false,
  wholeWord = false,
): Promise<SearchHit[]> {
  return invoke<SearchHit[]>('search_document', { id, query, matchCase, wholeWord });
}

export async function outline(id: number): Promise<OutlineNode[]> {
  return invoke<OutlineNode[]>('outline', { id });
}

export async function pageLinks(id: number, page: number): Promise<PageLink[]> {
  return invoke<PageLink[]>('page_links', { id, page });
}

export async function documentProperties(id: number): Promise<DocumentProperties> {
  return invoke<DocumentProperties>('document_properties', { id });
}

export async function fileSize(path: string): Promise<number> {
  return invoke<number>('file_size', { path });
}

/** Fichier PDF passé en argument au lancement de l'application, s'il y en a un. */
export async function startupFile(): Promise<string | null> {
  return invoke<string | null>('startup_file');
}

/** Ouvre le dialogue natif et retourne les chemins choisis (vide si annulé). */
export async function pickPdfFiles(): Promise<string[]> {
  const picked = await open({
    multiple: true,
    directory: false,
    filters: [{ name: 'PDF', extensions: ['pdf'] }],
  });
  if (picked === null) return [];
  return Array.isArray(picked) ? picked : [picked];
}
