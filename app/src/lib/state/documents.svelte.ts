// SPDX-License-Identifier: GPL-3.0-or-later
// État des documents ouverts (onglets) et actions associées. Toute décision sur un
// document passe par le backend ; ici on ne garde que ce qu'il faut pour l'afficher.

import {
  applyEdit as apiApplyEdit,
  closeDocument,
  extractPages,
  isAppError,
  mergeFiles,
  openDocument,
  pickSavePath,
  redoEdit,
  saveDocument,
  undoEdit,
  outline as fetchOutline,
  pageLinks,
  searchDocument,
  setGeneration,
  textLayer,
  type DocumentSummary,
  type EditCommand,
  type EditResult,
  type EditState,
  type OutlineNode,
  type PageLink,
  type Rotation,
  type SearchHit,
  type TextBox,
} from '$lib/api';
import { fileName } from '$lib/logic/labels';
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
  /** Historique d'édition (miroir de l'état du backend). */
  edit: EditState;
  /** Pages sélectionnées dans les vignettes (index 0). */
  selectedPages: number[];
}

export interface PasswordPrompt {
  path: string;
  error: string | null;
}

/** Demande de confirmation avant de fermer un document modifié. */
export interface ClosePrompt {
  id: number;
  /** Autres documents modifiés à confirmer ensuite (fermeture de la fenêtre). */
  queue: number[];
  /** À appeler quand toute la file est traitée (ex. fermer la fenêtre). */
  onDone: (() => void) | null;
}

interface DocsState {
  docs: OpenDoc[];
  activeId: number | null;
  passwordPrompt: PasswordPrompt | null;
  closePrompt: ClosePrompt | null;
  error: string | null;
  /** Message bref, non bloquant (ex. « Document enregistré. »). */
  notice: string | null;
  busy: boolean;
}

const UNTOUCHED: EditState = {
  modified: false,
  can_undo: false,
  can_redo: false,
  undo_label: null,
  redo_label: null,
};

export const docsState = $state<DocsState>({
  docs: [],
  activeId: null,
  passwordPrompt: null,
  closePrompt: null,
  error: null,
  notice: null,
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
      edit: { ...UNTOUCHED },
      selectedPages: [],
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

/** Ferme l'onglet, après confirmation si le document a des modifications en attente. */
export async function requestClose(id: number): Promise<void> {
  const doc = docById(id);
  if (!doc) return;
  if (!doc.edit.modified) {
    await closeDoc(id);
    return;
  }
  docsState.closePrompt = { id, queue: [], onDone: null };
}

/**
 * Fermeture de la fenêtre : vrai si rien n'est à confirmer (on peut fermer tout de suite).
 * Sinon, enchaîne une confirmation par document modifié puis appelle `onDone`.
 */
export function requestCloseAll(onDone: () => void): boolean {
  const modified = docsState.docs.filter((d) => d.edit.modified).map((d) => d.summary.id);
  if (modified.length === 0) return true;
  docsState.closePrompt = { id: modified[0], queue: modified.slice(1), onDone };
  return false;
}

/** Réponse au dialogue de fermeture. */
export async function resolveClosePrompt(choice: 'save' | 'discard' | 'cancel'): Promise<void> {
  const prompt = docsState.closePrompt;
  if (!prompt) return;
  if (choice === 'cancel') {
    docsState.closePrompt = null;
    return;
  }
  const doc = docById(prompt.id);
  if (doc && choice === 'save') {
    const ok = await save(doc);
    if (!ok) {
      // Enregistrement annulé ou en échec : on garde l'onglet et on arrête la file.
      docsState.closePrompt = null;
      return;
    }
  }
  await closeDoc(prompt.id);
  const [next, ...rest] = prompt.queue;
  if (next !== undefined) {
    docsState.closePrompt = { id: next, queue: rest, onDone: prompt.onDone };
  } else {
    docsState.closePrompt = null;
    prompt.onDone?.();
  }
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

// ---- Édition (Lot 2) ----

/**
 * Après toute édition : le résumé et l'état d'historique viennent du backend ; tout ce qui
 * dépend de la numérotation des pages (texte, liens, signets, recherche, sélection) est
 * oublié et sera rechargé ; une nouvelle génération force le rechargement des images.
 */
function afterEdit(doc: OpenDoc, result: EditResult): void {
  doc.summary = result.summary;
  doc.edit = result.state;
  doc.textLayers = {};
  doc.links = {};
  doc.outline = null;
  doc.selection = null;
  doc.search = null;
  doc.selectedPages = doc.selectedPages.filter((p) => p < result.summary.page_count);
  doc.page = Math.min(doc.page, Math.max(0, result.summary.page_count - 1));
  nextGeneration(doc);
}

function reportError(message: (m: string) => string, e: unknown): void {
  docsState.error = message(isAppError(e) ? e.message : String(e));
}

export async function applyEdit(doc: OpenDoc, command: EditCommand): Promise<boolean> {
  docsState.error = null;
  try {
    afterEdit(doc, await apiApplyEdit(doc.summary.id, command));
    return true;
  } catch (e) {
    reportError(t.editError, e);
    return false;
  }
}

export async function undo(doc: OpenDoc): Promise<void> {
  if (!doc.edit.can_undo) return;
  try {
    afterEdit(doc, await undoEdit(doc.summary.id));
  } catch (e) {
    reportError(t.editError, e);
  }
}

export async function redo(doc: OpenDoc): Promise<void> {
  if (!doc.edit.can_redo) return;
  try {
    afterEdit(doc, await redoEdit(doc.summary.id));
  } catch (e) {
    reportError(t.editError, e);
  }
}

/** Enregistre à son chemin ; un document sans chemin passe par « Enregistrer sous ». */
export async function save(doc: OpenDoc): Promise<boolean> {
  if (!doc.summary.path) return saveAs(doc);
  return writeTo(doc, undefined);
}

/** Choisit un chemin puis enregistre ; faux si l'utilisateur annule. */
export async function saveAs(doc: OpenDoc): Promise<boolean> {
  const path = await pickSavePath(doc.summary.path ?? undefined);
  if (!path) return false;
  return writeTo(doc, path);
}

async function writeTo(doc: OpenDoc, path: string | undefined): Promise<boolean> {
  docsState.error = null;
  try {
    const result = await saveDocument(doc.summary.id, path);
    // Enregistrer ne change ni les pages ni les images : seuls résumé et état bougent.
    doc.summary = result.summary;
    doc.edit = result.state;
    if (result.summary.path) rememberRecent(result.summary.path);
    showNotice(t.saved);
    return true;
  } catch (e) {
    reportError(t.saveError, e);
    return false;
  }
}

let noticeTimer: ReturnType<typeof setTimeout> | null = null;

export function showNotice(message: string): void {
  docsState.notice = message;
  if (noticeTimer) clearTimeout(noticeTimer);
  noticeTimer = setTimeout(() => (docsState.notice = null), 1500);
}

/** Extrait des pages vers un nouveau fichier ; l'erreur éventuelle passe par le bandeau. */
export async function extractTo(doc: OpenDoc, pages: number[], path: string): Promise<boolean> {
  docsState.error = null;
  try {
    await extractPages(doc.summary.id, pages, path);
    showNotice(t.extracted(pages.length));
    return true;
  } catch (e) {
    reportError(t.editError, e);
    return false;
  }
}

/** Nom affichable d'un document (pour les dialogues). */
export function displayName(doc: OpenDoc): string {
  return doc.summary.title || (doc.summary.path ? fileName(doc.summary.path) : t.untitled);
}

/** Fusionne des fichiers et ouvre le résultat (sans chemin : à enregistrer) dans un onglet. */
export async function mergeAndOpen(paths: string[]): Promise<boolean> {
  docsState.error = null;
  docsState.busy = true;
  try {
    const summary = await mergeFiles(paths);
    docsState.docs.push({
      summary,
      scale: clampScale(settings.scale),
      zoomMode: settings.zoomMode,
      rotation: 'none',
      page: 0,
      generation: summary.generation,
      scrollRequest: 0,
      outline: null,
      textLayers: {},
      links: {},
      selection: null,
      search: null,
      // Un document fusionné n'existe pas encore sur disque : il est « modifié » d'emblée.
      edit: { ...UNTOUCHED, modified: true },
      selectedPages: [],
    });
    docsState.activeId = summary.id;
    return true;
  } catch (e) {
    reportError(t.mergeError, e);
    return false;
  } finally {
    docsState.busy = false;
  }
}

export function setSelectedPages(doc: OpenDoc, pages: number[]): void {
  doc.selectedPages = pages;
}

export function clearError(): void {
  docsState.error = null;
}
