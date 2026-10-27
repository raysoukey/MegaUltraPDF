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

/** Une commande d'édition (miroir de `mega_core::EditCommand`, sérialisée avec `kind`). */
export type EditCommand =
  | { kind: 'rotate_pages'; pages: number[]; quarter_turns: number }
  | { kind: 'delete_pages'; pages: number[] }
  | { kind: 'move_pages'; pages: number[]; to: number }
  | { kind: 'insert_blank_page'; at: number; size: PageSize }
  | { kind: 'insert_pages_from_file'; path: string; pages: number[] | null; at: number }
  | { kind: 'add_annotation'; page: number; annotation: NewAnnotation }
  | { kind: 'delete_annotation'; page: number; index: number }
  | { kind: 'update_annotation'; page: number; index: number; annotation: NewAnnotation }
  | { kind: 'move_annotation'; page: number; index: number; rect: Rect };

/** Genre d'annotation (miroir de `AnnotationKind`). */
export type AnnotationKind =
  | 'note'
  | 'highlight'
  | 'underline'
  | 'strike_out'
  | 'squiggly'
  | 'rect'
  | 'ellipse'
  | 'line'
  | 'arrow'
  | 'ink'
  | 'text'
  | 'stamp'
  | 'link'
  | 'widget'
  | 'other';

/** Couleur opaque (miroir de `Color`). */
export interface Color {
  r: number;
  g: number;
  b: number;
}

/** Style d'une annotation à créer (miroir de `Style`). */
export interface Style {
  color: Color;
  fill: Color | null;
  opacity: number;
  stroke_width: number;
  font_size: number;
}

/** Géométrie en points PDF (miroir de `Geometry`, sérialisée avec `kind`). */
export type Geometry =
  | { kind: 'rect'; rect: Rect }
  | { kind: 'quads'; rects: Rect[] }
  | { kind: 'line'; from: [number, number]; to: [number, number] }
  | { kind: 'strokes'; strokes: [number, number][][] };

/** Une annotation à créer (miroir de `NewAnnotation`). */
export interface NewAnnotation {
  kind: AnnotationKind;
  geometry: Geometry;
  style: Style;
  contents: string;
  author: string;
}

/** Une annotation lue (miroir de `Annotation`). */
export interface Annotation {
  index: number;
  kind: AnnotationKind;
  rect: Rect;
  color: Color | null;
  fill: Color | null;
  opacity: number;
  stroke_width: number;
  font_size: number | null;
  author: string | null;
  contents: string | null;
  created: string | null;
  modified: string | null;
  quads: Rect[];
  geometry: Geometry | null;
}

/** État de l'historique (miroir de `EditState`). */
export interface EditState {
  modified: boolean;
  can_undo: boolean;
  can_redo: boolean;
  undo_label: string | null;
  redo_label: string | null;
}

/** Résultat d'une édition (miroir de `EditResult`). */
export interface EditResult {
  summary: DocumentSummary;
  state: EditState;
}

/** Erreur renvoyée par le backend (miroir de `AppError`). */
export interface AppError {
  kind:
    | 'password_required'
    | 'wrong_password'
    | 'invalid_pdf'
    | 'not_found'
    | 'invalid_operation'
    | 'io'
    | 'internal';
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

export async function annotations(id: number, page: number): Promise<Annotation[]> {
  return invoke<Annotation[]>('annotations', { id, page });
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

export async function applyEdit(id: number, command: EditCommand): Promise<EditResult> {
  return invoke<EditResult>('apply_edit', { id, command });
}

export async function undoEdit(id: number): Promise<EditResult> {
  return invoke<EditResult>('undo_edit', { id });
}

export async function redoEdit(id: number): Promise<EditResult> {
  return invoke<EditResult>('redo_edit', { id });
}

export async function editState(id: number): Promise<EditState> {
  return invoke<EditState>('edit_state', { id });
}

/** Enregistre à son chemin, ou dans `path` (« Enregistrer sous »). */
export async function saveDocument(id: number, path?: string): Promise<EditResult> {
  return invoke<EditResult>('save_document', { id, path: path ?? null });
}

export async function extractPages(id: number, pages: number[], path: string): Promise<void> {
  await invoke('extract_pages', { id, pages, path });
}

export async function mergeFiles(paths: string[]): Promise<DocumentSummary> {
  return invoke<DocumentSummary>('merge_files', { paths });
}

export async function splitDocument(
  id: number,
  parts: number[][],
  outDir: string,
  baseName: string,
): Promise<string[]> {
  return invoke<string[]>('split_document', { id, parts, outDir, baseName });
}

/** Façon de scinder (miroir de `SplitMode`, sérialisée avec `kind`). */
export type SplitMode =
  { kind: 'ranges'; spec: string } | { kind: 'every'; n: number } | { kind: 'bookmarks' };

/** Les parties qu'une scission produirait (index de pages), sans rien écrire. */
export async function splitPlan(id: number, mode: SplitMode): Promise<number[][]> {
  return invoke<number[][]>('split_plan', { id, mode });
}

/** Dialogue natif « Enregistrer sous » ; `null` si annulé. */
export async function pickSavePath(defaultPath?: string): Promise<string | null> {
  const { save } = await import('@tauri-apps/plugin-dialog');
  return save({ defaultPath, filters: [{ name: 'PDF', extensions: ['pdf'] }] });
}

/** Dialogue natif de choix d'un dossier ; `null` si annulé. */
export async function pickDirectory(): Promise<string | null> {
  const picked = await open({ directory: true, multiple: false });
  return typeof picked === 'string' ? picked : null;
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
