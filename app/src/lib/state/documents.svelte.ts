// SPDX-License-Identifier: GPL-3.0-or-later
// État des documents ouverts (onglets) et actions associées. Toute décision sur un
// document passe par le backend ; ici on ne garde que ce qu'il faut pour l'afficher.

import {
  closeDocument,
  isAppError,
  openDocument,
  outline as fetchOutline,
  pageLinks,
  searchDocument,
  setGeneration,
  textLayer,
  type DocumentSummary,
  type OutlineNode,
  type PageLink,
  type Rotation,
  type SearchHit,
  type TextBox,
} from '$lib/api';
import { pagesToEvict } from '$lib/logic/pagecache';
import { clampScale } from '$lib/logic/zoom';
import { t } from '$lib/strings';
import {
  forgetRecent,
  rememberPosition,
  rememberRecent,
  saveSettings,
  settings,
  type ZoomMode,
} from './settings.svelte';

/** Sélection de texte courante : une plage de caractères sur une page. */
export interface TextSelection {
  page: number;
  from: number;
  to: number;
}

/** Un document ouvert et son état de visualisation. */
export interface OpenDoc {
  summary: DocumentSummary;
  scale: number;
  zoomMode: ZoomMode;
  rotation: Rotation;
  page: number;
  generation: number;
  /** Demande de défilement vers une page (consommée par la visionneuse). */
  scrollRequest: number | null;
  outline: OutlineNode[] | null;
  textLayers: Record<number, TextBox[]>;
  links: Record<number, PageLink[]>;
  selection: TextSelection | null;
  search: { query: string; hits: SearchHit[]; current: number } | null;
}

export interface PasswordPrompt {
  path: string;
  error: string | null;
}

interface DocsState {
  docs: OpenDoc[];
  activeId: number | null;
  passwordPrompt: PasswordPrompt | null;
  error: string | null;
  busy: boolean;
}

export const docsState = $state<DocsState>({
  docs: [],
  activeId: null,
  passwordPrompt: null,
  error: null,
  busy: false,
});

export function activeDoc(): OpenDoc | null {
  return docsState.docs.find((d) => d.summary.id === docsState.activeId) ?? null;
}

export function docById(id: number): OpenDoc | null {
  return docsState.docs.find((d) => d.summary.id === id) ?? null;
}

function initialScaleFor(path: string | null): { scale: number; zoomMode: ZoomMode; page: number } {
  const remembered = path ? settings.positions[path] : undefined;
  if (remembered)
    return { scale: remembered.scale, zoomMode: remembered.zoomMode, page: remembered.page };
  return { scale: settings.scale, zoomMode: settings.zoomMode, page: 0 };
}

/** Ouvre un fichier ; en cas de mot de passe requis, affiche le dialogue dédié. */
export async function openPath(path: string, password?: string): Promise<boolean> {
  docsState.error = null;
  const already = docsState.docs.find((d) => d.summary.path === path);
  if (already) {
    docsState.activeId = already.summary.id;
    return true;
  }
  docsState.busy = true;
  try {
    const summary = await openDocument(path, password);
    const initial = initialScaleFor(summary.path);
    docsState.docs.push({
      summary,
      scale: clampScale(initial.scale),
      zoomMode: initial.zoomMode,
      rotation: 'none',
      page: Math.min(initial.page, Math.max(0, summary.page_count - 1)),
      generation: summary.generation,
      scrollRequest: initial.page,
      outline: null,
      textLayers: {},
      links: {},
      selection: null,
      search: null,
    });
    docsState.activeId = summary.id;
    docsState.passwordPrompt = null;
    if (summary.path) rememberRecent(summary.path);
    return true;
  } catch (e) {
    if (isAppError(e)) {
      if (e.kind === 'password_required') {
        docsState.passwordPrompt = { path, error: null };
      } else if (e.kind === 'wrong_password') {
        docsState.passwordPrompt = { path, error: t.wrongPassword };
      } else {
        if (e.kind === 'io') forgetRecent(path);
        docsState.error = t.openError(e.message);
      }
    } else {
      docsState.error = t.openError(String(e));
    }
    return false;
  } finally {
    docsState.busy = false;
  }
}

export async function openPaths(paths: string[]): Promise<void> {
  for (const p of paths) {
    if (p.toLowerCase().endsWith('.pdf')) await openPath(p);
  }
}

export function cancelPassword(): void {
  docsState.passwordPrompt = null;
}

export async function closeDoc(id: number): Promise<void> {
  const index = docsState.docs.findIndex((d) => d.summary.id === id);
  if (index === -1) return;
  const doc = docsState.docs[index];
  if (doc.summary.path) {
    rememberPosition(doc.summary.path, {
      page: doc.page,
      scale: doc.scale,
      zoomMode: doc.zoomMode,
    });
  }
  docsState.docs.splice(index, 1);
  if (docsState.activeId === id) {
    const neighbour = docsState.docs[Math.min(index, docsState.docs.length - 1)];
    docsState.activeId = neighbour ? neighbour.summary.id : null;
  }
  await closeDocument(id).catch(() => undefined);
}

export function activate(id: number): void {
  if (docById(id)) docsState.activeId = id;
}

/** Change l'échelle (et le mode) ; une nouvelle génération invalide les rendus en cours. */
/**
 * Nouvelle génération, décidée ici de façon **synchrone** : les rendus demandés ensuite
 * portent déjà le bon numéro, et deux changements rapprochés ne peuvent pas se doubler.
 * Le backend est simplement informé (il ne recule jamais, voir `set_generation`).
 */
function nextGeneration(doc: OpenDoc): void {
  doc.generation += 1;
  void setGeneration(doc.summary.id, doc.generation).catch(() => undefined);
}

export function setScale(doc: OpenDoc, scale: number, zoomMode: ZoomMode): void {
  const next = clampScale(scale);
  const changed = Math.abs(next - doc.scale) > 1e-6;
  doc.zoomMode = zoomMode;
  settings.zoomMode = zoomMode;
  if (changed) {
    nextGeneration(doc);
    doc.scale = next;
    settings.scale = next;
  }
  saveSettings();
}

export function rotateDoc(doc: OpenDoc): void {
  const order: Rotation[] = ['none', 'cw90', 'cw180', 'cw270'];
  nextGeneration(doc);
  doc.rotation = order[(order.indexOf(doc.rotation) + 1) % 4];
}

/** Mémorise la position de lecture de tous les onglets (fermeture de la fenêtre). */
export function rememberAllPositions(): void {
  for (const doc of docsState.docs) {
    if (doc.summary.path) {
      rememberPosition(doc.summary.path, {
        page: doc.page,
        scale: doc.scale,
        zoomMode: doc.zoomMode,
      });
    }
  }
}

/** Borne des caches par page (couche de texte, liens) : pages les plus proches gardées. */
const PAGE_CACHE_KEEP = 24;

function pruneRecord<T>(record: Record<number, T>, current: number): void {
  const cached = Object.keys(record).map(Number);
  for (const page of pagesToEvict(cached, current, PAGE_CACHE_KEEP)) delete record[page];
}

export function goToPage(doc: OpenDoc, page: number): void {
  const clamped = Math.max(0, Math.min(doc.summary.page_count - 1, page));
  doc.page = clamped;
  doc.scrollRequest = clamped;
}

export async function ensureOutline(doc: OpenDoc): Promise<void> {
  if (doc.outline !== null) return;
  doc.outline = await fetchOutline(doc.summary.id).catch(() => []);
}

export async function ensureTextLayer(doc: OpenDoc, page: number): Promise<TextBox[]> {
  const cached = doc.textLayers[page];
  if (cached) return cached;
  const boxes = await textLayer(doc.summary.id, page).catch(() => []);
  doc.textLayers[page] = boxes;
  pruneRecord(doc.textLayers, doc.page);
  return boxes;
}

export async function ensureLinks(doc: OpenDoc, page: number): Promise<PageLink[]> {
  const cached = doc.links[page];
  if (cached) return cached;
  const links = await pageLinks(doc.summary.id, page).catch(() => []);
  doc.links[page] = links;
  pruneRecord(doc.links, doc.page);
  return links;
}

let searchSequence = 0;

export async function runSearch(doc: OpenDoc, query: string, matchCase: boolean): Promise<void> {
  if (query.trim() === '') {
    doc.search = null;
    return;
  }
  // Deux recherches en vol peuvent répondre dans le désordre : seule la dernière compte.
  const ticket = ++searchSequence;
  const hits = await searchDocument(doc.summary.id, query, matchCase).catch(() => []);
  if (ticket !== searchSequence) return;
  doc.search = { query, hits, current: hits.length > 0 ? 0 : -1 };
  if (hits.length > 0) goToPage(doc, hits[0].page);
}

export function stepSearch(doc: OpenDoc, dir: 1 | -1): void {
  if (!doc.search || doc.search.hits.length === 0) return;
  const n = doc.search.hits.length;
  doc.search.current = (doc.search.current + dir + n) % n;
  goToPage(doc, doc.search.hits[doc.search.current].page);
}

/** Fixe (ou efface) la sélection de texte. Passe par ici plutôt que de muter `doc` depuis
 *  un composant enfant : Svelte signale les mutations de props non liées. */
export function setSelection(doc: OpenDoc, selection: TextSelection | null): void {
  doc.selection = selection;
}

/** Change la page courante sans demander de défilement (appelé par la visionneuse). */
export function notePage(doc: OpenDoc, page: number): void {
  doc.page = page;
}

/** Marque la demande de défilement comme traitée. */
export function consumeScrollRequest(doc: OpenDoc): void {
  doc.scrollRequest = null;
}

export function clearSearch(doc: OpenDoc): void {
  searchSequence++;
  doc.search = null;
}
