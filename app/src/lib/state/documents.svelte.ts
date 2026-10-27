// SPDX-License-Identifier: GPL-3.0-or-later
// État des documents ouverts (onglets) et actions associées. Toute décision sur un
// document passe par le backend ; ici on ne garde que ce qu'il faut pour l'afficher.

import {
  allAnnotations as fetchAllAnnotations,
  allFormFields as fetchAllFormFields,
  annotations as fetchAnnotations,
  attachments as fetchAttachments,
  signatures as fetchSignatures,
  signDocument as apiSignDocument,
  pagesWithText as fetchPagesWithText,
  applyEdit as apiApplyEdit,
  exportImages as apiExportImages,
  exportText as apiExportText,
  pickAnyFile,
  pickSaveFile,
  saveAttachment as apiSaveAttachment,
  security as fetchSecurity,
  formFields as fetchFormFields,
  formInfo as fetchFormInfo,
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
  pageObjects as fetchPageObjects,
  pickImageFile,
  searchDocument,
  setGeneration,
  textLayer,
  type Annotation,
  type AnnotationKind,
  type AnnotationPatch,
  type DocumentSummary,
  type EditCommand,
  type FieldValue,
  type FormField,
  type FormInfo,
  type NewAnnotation,
  type Attachment,
  type ImageExport,
  type MetadataPatch,
  type OptimizeOptions,
  type Protection,
  type SecurityInfo,
  type SignatureInfo,
  type SignOptions,
  type CropSpec,
  type PageNumbering,
  type PageObject,
  type SignatureSource,
  type TextStyle,
  type Watermark,
  type Rect,
  type EditResult,
  type EditState,
  type OutlineNode,
  type PageLink,
  type Rotation,
  type SearchHit,
  type TextBox,
} from '$lib/api';
import { quadsFromSelection, selectionAfterMove } from '$lib/logic/annotations';
import { fileName } from '$lib/logic/labels';
import { initialPosition } from '$lib/logic/position';
import { openErrorMessage } from '$lib/logic/errors';
import { selectionRects } from '$lib/logic/selection';
import { pagesToEvict } from '$lib/logic/pagecache';
import { clampScale } from '$lib/logic/zoom';
import { serialQueue } from '$lib/logic/queue';
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
  /** Annotations par page (cache vidé à chaque édition). */
  annotations: Record<number, Annotation[]>;
  /** Annotation sélectionnée dans le calque ou le panneau. */
  selectedAnnotation: { page: number; index: number } | null;
  /** Champs de formulaire par page (cache vidé à chaque édition). */
  formFields: Record<number, FormField[]>;
  /** Nature du formulaire (chargée à l'ouverture, rafraîchie après édition). */
  formInfo: FormInfo | null;
  /** Zone tracée pour une signature, en attente du dialogue. */
  signatureRequest: { page: number; rect: Rect } | null;
  /** Objets du contenu par page (cache vidé à chaque édition), Lot 5. */
  pageObjects: Record<number, PageObject[]>;
  /** Objet de contenu sélectionné (outil Objets). */
  selectedObject: { page: number; index: number } | null;
  /** Point cliqué avec l'outil Texte-contenu, en attente du dialogue. */
  textInsertRequest: { page: number; at: [number, number] } | null;
  /** Zone tracée avec l'outil Caviarder, en attente de confirmation. */
  redactRequest: { page: number; rect: Rect } | null;
  /** État de sécurité (chargé à la demande, rafraîchi après édition), Lot 6. */
  security: SecurityInfo | null;
  /** Pièces jointes (chargées à la demande, rafraîchies après édition). */
  attachments: Attachment[] | null;
  /**
   * Signatures numériques du **fichier** (Lot 8), chargées à l'ouverture. Une édition ne
   * les change pas (elles portent sur le fichier) ; un enregistrement les fait relire.
   */
  signatures: SignatureInfo[] | null;
  /**
   * État de vue gardé avec le document (et non dans les composants) : il survit au
   * remontage de l'interface lors d'un changement de langue (C32).
   */
  view: DocView;
}

/** Onglet de la barre latérale. */
export type SidebarTab =
  'thumbnails' | 'bookmarks' | 'annotations' | 'form' | 'attachments' | 'signatures';

/** Ce que l'utilisateur a choisi dans la vue d'un document. */
export interface DocView {
  sidebarTab: SidebarTab;
  /** Recherche sensible à la casse. */
  matchCase: boolean;
  /** Signets dépliés ou repliés, par clé de signet. */
  outlineOpen: Record<string, boolean>;
}

/** Vue d'un document qui vient d'être ouvert. */
function freshView(): DocView {
  return { sidebarTab: 'thumbnails', matchCase: false, outlineOpen: {} };
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
    const initial = initialPosition(settings, summary.path);
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
      annotations: {},
      selectedAnnotation: null,
      formFields: {},
      formInfo: null,
      signatureRequest: null,
      pageObjects: {},
      selectedObject: null,
      textInsertRequest: null,
      redactRequest: null,
      security: null,
      attachments: null,
      signatures: null,
      view: freshView(),
    });
    docsState.activeId = summary.id;
    docsState.passwordPrompt = null;
    // Signatures du fichier lues dès l'ouverture : le bandeau « document signé » en dépend.
    const opened = docsState.docs.find((d) => d.summary.id === summary.id);
    if (opened) void ensureSignatures(opened);
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
        docsState.error = openErrorMessage(e);
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
  await flushPendingInput();
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
  doc.annotations = {};
  doc.selectedAnnotation = null;
  doc.formFields = {};
  doc.formInfo = null;
  doc.pageObjects = {};
  // Les index d'objets sont positionnels : une sélection survivant à une édition pourrait
  // désigner un autre objet (annuler une insertion, caviarder). Les actions qui veulent
  // garder la sélection (déplacer, modifier le texte) la reposent après coup.
  doc.selectedObject = null;
  doc.security = null;
  doc.attachments = null;
  doc.page = Math.min(doc.page, Math.max(0, result.summary.page_count - 1));
  nextGeneration(doc);
}

function reportError(message: (m: string) => string, e: unknown): void {
  docsState.error = message(isAppError(e) ? e.message : String(e));
}

/** Éditions envoyées au backend dont la réponse n'est pas encore arrivée. */
let inFlight: Promise<unknown>[] = [];

/** Attend la fin des éditions en vol (avant d'enregistrer ou de fermer). */
export async function settleEdits(): Promise<void> {
  while (inFlight.length > 0) await Promise.allSettled([...inFlight]);
}

/**
 * Éditions, annulations et rétablissements passent **l'un après l'autre** (A23) : deux Ctrl+Z
 * rapides annulent deux fois, le second voyant l'état laissé par le premier, au lieu
 * d'afficher « rien à annuler ». Chaque opération est aussi attendue par l'enregistrement
 * et la fermeture (`inFlight`).
 */
const editQueue = serialQueue();

function queued<T>(task: () => Promise<T>): Promise<T> {
  const pending = editQueue(task);
  inFlight.push(pending);
  void pending
    .catch(() => undefined)
    .finally(() => (inFlight = inFlight.filter((p) => p !== pending)));
  return pending;
}

export async function applyEdit(doc: OpenDoc, command: EditCommand): Promise<boolean> {
  return queued(async () => {
    docsState.error = null;
    try {
      afterEdit(doc, await apiApplyEdit(doc.summary.id, command));
      return true;
    } catch (e) {
      reportError(t.editError, e);
      return false;
    }
  });
}

/** Valide le champ en cours de saisie (son `change` part au blur) puis attend les éditions. */
async function flushPendingInput(): Promise<void> {
  const active = typeof document === 'undefined' ? null : document.activeElement;
  if (active instanceof HTMLElement && active.closest('.form-layer')) active.blur();
  await settleEdits();
}

export async function undo(doc: OpenDoc): Promise<void> {
  // L'état est relu **dans** la file : après une annulation précédente, pas avant.
  await queued(async () => {
    if (!doc.edit.can_undo) return;
    try {
      afterEdit(doc, await undoEdit(doc.summary.id));
    } catch (e) {
      reportError(t.editError, e);
    }
  });
}

export async function redo(doc: OpenDoc): Promise<void> {
  await queued(async () => {
    if (!doc.edit.can_redo) return;
    try {
      afterEdit(doc, await redoEdit(doc.summary.id));
    } catch (e) {
      reportError(t.editError, e);
    }
  });
}

/** Enregistre à son chemin ; un document sans chemin passe par « Enregistrer sous ». */
export async function save(doc: OpenDoc): Promise<boolean> {
  await flushPendingInput();
  if (!doc.summary.path) return saveAs(doc);
  return writeTo(doc, undefined);
}

/** Choisit un chemin puis enregistre ; faux si l'utilisateur annule. */
export async function saveAs(doc: OpenDoc): Promise<boolean> {
  await flushPendingInput();
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
    // Le fichier a changé : ses signatures sont à relire.
    void refreshSignatures(doc);
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
      annotations: {},
      selectedAnnotation: null,
      formFields: {},
      formInfo: null,
      signatureRequest: null,
      pageObjects: {},
      selectedObject: null,
      textInsertRequest: null,
      redactRequest: null,
      security: null,
      attachments: null,
      signatures: null,
      view: freshView(),
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

// ---- Annotations (Lot 3) ----

export async function ensureAnnotations(doc: OpenDoc, page: number): Promise<Annotation[]> {
  const cached = doc.annotations[page];
  if (cached) return cached;
  try {
    const list = await fetchAnnotations(doc.summary.id, page);
    doc.annotations[page] = list;
    return list;
  } catch (e) {
    // Rien en cache : la prochaine lecture réessaiera (B27).
    reportError(t.editError, e);
    return [];
  }
}

/**
 * Annotations de toutes les pages (panneau des annotations) : une seule requête, qui remplit
 * le cache par page. En cas d'erreur, rien n'est mis en cache (l'erreur est affichée).
 */
export async function ensureAllAnnotations(doc: OpenDoc): Promise<Annotation[][]> {
  const count = doc.summary.page_count;
  const cached = Array.from({ length: count }, (_, p) => doc.annotations[p]);
  if (cached.every((list) => list !== undefined)) return cached as Annotation[][];
  try {
    const all = await fetchAllAnnotations(doc.summary.id);
    all.forEach((list, p) => (doc.annotations[p] = list));
    return all;
  } catch (e) {
    reportError(t.editError, e);
    return [];
  }
}

export function selectAnnotation(
  doc: OpenDoc,
  target: { page: number; index: number } | null,
): void {
  doc.selectedAnnotation = target;
}

/** Ajoute une annotation ; la nouvelle annotation (en fin de tableau) devient sélectionnée. */
export async function addAnnotation(
  doc: OpenDoc,
  page: number,
  annotation: NewAnnotation,
): Promise<boolean> {
  const before = (doc.annotations[page] ?? (await ensureAnnotations(doc, page))).length;
  const ok = await applyEdit(doc, { kind: 'add_annotation', page, annotation });
  if (ok) doc.selectedAnnotation = { page, index: before };
  return ok;
}

export async function deleteAnnotation(
  doc: OpenDoc,
  page: number,
  index: number,
): Promise<boolean> {
  return applyEdit(doc, { kind: 'delete_annotation', page, index });
}

export async function updateAnnotation(
  doc: OpenDoc,
  page: number,
  index: number,
  patch: AnnotationPatch,
): Promise<boolean> {
  const kind = (doc.annotations[page] ?? []).find((a) => a.index === index)?.kind ?? 'other';
  const ok = await applyEdit(doc, { kind: 'update_annotation', page, index, patch });
  if (ok) {
    const list = await ensureAnnotations(doc, page);
    doc.selectedAnnotation = { page, index: selectionAfterMove(kind, index, list.length) };
  }
  return ok;
}

/** Déplace / redimensionne ; l'annotation recréée (formes, texte) finit en fin de tableau. */
export async function moveAnnotation(
  doc: OpenDoc,
  page: number,
  index: number,
  rect: Rect,
): Promise<boolean> {
  // Le genre est lu **avant** l'édition : après, les index ont pu changer.
  const kind = (doc.annotations[page] ?? []).find((a) => a.index === index)?.kind ?? 'other';
  const ok = await applyEdit(doc, { kind: 'move_annotation', page, index, rect });
  if (ok) {
    const list = await ensureAnnotations(doc, page);
    doc.selectedAnnotation = { page, index: selectionAfterMove(kind, index, list.length) };
  }
  return ok;
}

/** Marque le texte sélectionné (surligner, souligner, barrer). */
export async function annotateSelection(
  doc: OpenDoc,
  kind: AnnotationKind,
  style: NewAnnotation['style'],
  author: string,
): Promise<boolean> {
  const sel = doc.selection;
  if (!sel) return false;
  const boxes = doc.textLayers[sel.page] ?? [];
  const rects = quadsFromSelection(selectionRects(boxes, sel.from, sel.to));
  if (rects.length === 0) return false;
  const ok = await addAnnotation(doc, sel.page, {
    kind,
    geometry: { kind: 'quads', rects },
    style,
    contents: '',
    author,
  });
  if (ok) doc.selection = null;
  return ok;
}

/** Supprime l'annotation sélectionnée, s'il y en a une. */
export async function deleteSelectedAnnotation(doc: OpenDoc): Promise<void> {
  const sel = doc.selectedAnnotation;
  if (!sel) return;
  await deleteAnnotation(doc, sel.page, sel.index);
}

// ---- Formulaires (Lot 4) ----

export async function ensureFormInfo(doc: OpenDoc): Promise<FormInfo | null> {
  if (doc.formInfo) return doc.formInfo;
  try {
    const info = await fetchFormInfo(doc.summary.id);
    doc.formInfo = info;
    return info;
  } catch (e) {
    reportError(t.editError, e);
    return null;
  }
}

export async function ensureFormFields(doc: OpenDoc, page: number): Promise<FormField[]> {
  const cached = doc.formFields[page];
  if (cached) return cached;
  try {
    const list = await fetchFormFields(doc.summary.id, page);
    doc.formFields[page] = list;
    return list;
  } catch (e) {
    reportError(t.editError, e);
    return [];
  }
}

/** Champs de toutes les pages (panneau du formulaire) : une seule requête, sans cache d'erreur. */
export async function ensureAllFormFields(doc: OpenDoc): Promise<FormField[][]> {
  const count = doc.summary.page_count;
  const cached = Array.from({ length: count }, (_, p) => doc.formFields[p]);
  if (cached.every((list) => list !== undefined)) return cached as FormField[][];
  try {
    const all = await fetchAllFormFields(doc.summary.id);
    all.forEach((list, p) => (doc.formFields[p] = list));
    return all;
  } catch (e) {
    reportError(t.editError, e);
    return [];
  }
}

export async function setFieldValue(
  doc: OpenDoc,
  page: number,
  index: number,
  value: FieldValue,
): Promise<boolean> {
  return applyEdit(doc, { kind: 'set_field_value', page, index, value });
}

export async function flattenForm(doc: OpenDoc): Promise<boolean> {
  return applyEdit(doc, { kind: 'flatten_form' });
}

export function requestSignature(doc: OpenDoc, page: number, rect: Rect): void {
  doc.signatureRequest = { page, rect };
}

export function cancelSignature(doc: OpenDoc): void {
  doc.signatureRequest = null;
}

export async function insertSignature(
  doc: OpenDoc,
  page: number,
  rect: Rect,
  source: SignatureSource,
): Promise<boolean> {
  return applyEdit(doc, { kind: 'insert_signature', page, rect, source });
}

// ---- Contenu (Lot 5) ----

export async function ensureObjects(doc: OpenDoc, page: number): Promise<PageObject[]> {
  const cached = doc.pageObjects[page];
  if (cached) return cached;
  try {
    const list = await fetchPageObjects(doc.summary.id, page);
    doc.pageObjects[page] = list;
    return list;
  } catch (e) {
    reportError(t.editError, e);
    return [];
  }
}

export function selectObject(doc: OpenDoc, target: { page: number; index: number } | null): void {
  doc.selectedObject = target;
}

/** Supprime l'objet sélectionné ; les index suivants reculent, la sélection est levée. */
export async function deleteSelectedObject(doc: OpenDoc): Promise<void> {
  const sel = doc.selectedObject;
  if (!sel) return;
  doc.selectedObject = null;
  await applyEdit(doc, { kind: 'delete_object', page: sel.page, index: sel.index });
}

/** Déplace un objet de `(dx, dy)` points affichés ; son index ne change pas. */
/** Redimensionne l'objet `index` de la page `page` dans `rect` (points affichés). */
export async function resizeObject(
  doc: OpenDoc,
  page: number,
  index: number,
  rect: Rect,
): Promise<boolean> {
  const ok = await applyEdit(doc, { kind: 'resize_object', page, index, rect });
  if (ok) doc.selectedObject = { page, index };
  return ok;
}

export async function moveObject(
  doc: OpenDoc,
  page: number,
  index: number,
  dx: number,
  dy: number,
): Promise<boolean> {
  const ok = await applyEdit(doc, { kind: 'move_object', page, index, dx, dy });
  if (ok) doc.selectedObject = { page, index };
  return ok;
}

export async function setObjectText(
  doc: OpenDoc,
  page: number,
  index: number,
  text: string,
): Promise<boolean> {
  const ok = await applyEdit(doc, { kind: 'set_object_text', page, index, text });
  if (ok) doc.selectedObject = { page, index };
  return ok;
}

export function requestTextInsert(doc: OpenDoc, page: number, at: [number, number]): void {
  doc.textInsertRequest = { page, at };
}

export function cancelTextInsert(doc: OpenDoc): void {
  doc.textInsertRequest = null;
}

export async function insertText(
  doc: OpenDoc,
  page: number,
  at: [number, number],
  text: string,
  style: TextStyle,
): Promise<boolean> {
  const ok = await applyEdit(doc, { kind: 'insert_text', page, at, text, style });
  if (ok) doc.textInsertRequest = null;
  return ok;
}

/** Zone tracée avec l'outil Image : choix du fichier, puis insertion ajustée dans la zone. */
export async function insertImageAt(doc: OpenDoc, page: number, rect: Rect): Promise<boolean> {
  const path = await pickImageFile();
  if (!path) return false;
  return applyEdit(doc, { kind: 'insert_image', page, rect, path });
}

export function requestRedact(doc: OpenDoc, page: number, rect: Rect): void {
  doc.redactRequest = { page, rect };
}

export function cancelRedact(doc: OpenDoc): void {
  doc.redactRequest = null;
}

export async function confirmRedact(doc: OpenDoc): Promise<boolean> {
  const req = doc.redactRequest;
  if (!req) return false;
  doc.redactRequest = null;
  const ok = await applyEdit(doc, { kind: 'redact', page: req.page, rect: req.rect });
  if (ok) showNotice(t.redacted);
  return ok;
}

export async function addWatermark(doc: OpenDoc, spec: Watermark): Promise<boolean> {
  return applyEdit(doc, { kind: 'watermark', spec });
}

export async function addPageNumbers(doc: OpenDoc, spec: PageNumbering): Promise<boolean> {
  return applyEdit(doc, { kind: 'page_numbers', spec });
}

export async function cropPages(doc: OpenDoc, spec: CropSpec): Promise<boolean> {
  return applyEdit(doc, { kind: 'crop_pages', spec });
}

// ---- Export, sécurité, pièces jointes (Lot 6) ----

export async function ensureSecurity(doc: OpenDoc): Promise<SecurityInfo | null> {
  if (doc.security) return doc.security;
  const info = await fetchSecurity(doc.summary.id).catch(() => null);
  doc.security = info;
  return info;
}

export async function ensureAttachments(doc: OpenDoc): Promise<Attachment[]> {
  if (doc.attachments) return doc.attachments;
  const list = await fetchAttachments(doc.summary.id).catch(() => []);
  doc.attachments = list;
  return list;
}

/** Signatures du fichier (Lot 8), lues une fois puis gardées. */
export async function ensureSignatures(doc: OpenDoc): Promise<SignatureInfo[]> {
  if (doc.signatures) return doc.signatures;
  const list = await fetchSignatures(doc.summary.id).catch(() => []);
  doc.signatures = list;
  return list;
}

/** Relit les signatures après un enregistrement ou une signature. */
export async function refreshSignatures(doc: OpenDoc): Promise<void> {
  doc.signatures = null;
  await ensureSignatures(doc);
}

/** Pour chaque page, contient-elle déjà du texte ? `null` en cas d'erreur (à signaler). */
export async function pagesWithText(doc: OpenDoc): Promise<boolean[] | null> {
  return fetchPagesWithText(doc.summary.id).catch(() => null);
}

/** Reconnaît le texte d'une page (OCR) ; faux en cas d'échec (erreur affichée). */
export async function ocrPage(doc: OpenDoc, page: number, languages: string[]): Promise<boolean> {
  return applyEdit(doc, { kind: 'ocr_page', page, languages });
}

/**
 * Signe le document et l'enregistre dans un fichier choisi. Retourne `null` si c'est fait,
 * `''` si l'utilisateur a annulé le choix du fichier, sinon le message d'erreur à afficher
 * dans le dialogue (mot de passe incorrect, certificat illisible…).
 */
export async function signAndSave(
  doc: OpenDoc,
  p12: string,
  password: string,
  options: SignOptions,
): Promise<string | null> {
  await flushPendingInput();
  const path = await pickSavePath(doc.summary.path ?? undefined);
  if (!path) return '';
  try {
    const result = await apiSignDocument(doc.summary.id, path, p12, password, options);
    doc.summary = result.summary;
    doc.edit = result.state;
    if (result.summary.path) rememberRecent(result.summary.path);
    await refreshSignatures(doc);
    showNotice(t.signDone);
    return null;
  } catch (e) {
    if (isAppError(e) && e.kind === 'wrong_password') return t.wrongPassword;
    return t.signError(isAppError(e) ? e.message : String(e));
  }
}

export async function setMetadata(doc: OpenDoc, patch: MetadataPatch): Promise<boolean> {
  return applyEdit(doc, { kind: 'set_metadata', patch });
}

export async function protectDoc(doc: OpenDoc, protection: Protection): Promise<boolean> {
  const ok = await applyEdit(doc, { kind: 'protect', protection });
  if (ok) showNotice(t.protected);
  return ok;
}

/** Retire la protection ; `password` saisi (fichier à mot de passe propriétaire seul) ou
 *  `null` pour celui de l'ouverture. */
export async function unprotectDoc(doc: OpenDoc, password: string | null = null): Promise<boolean> {
  const ok = await applyEdit(doc, { kind: 'unprotect', password });
  if (ok) showNotice(t.unprotected);
  return ok;
}

export async function optimizeDoc(doc: OpenDoc, options: OptimizeOptions): Promise<boolean> {
  return applyEdit(doc, { kind: 'optimize', options });
}

export async function deleteAttachment(doc: OpenDoc, index: number): Promise<boolean> {
  return applyEdit(doc, { kind: 'delete_attachment', index });
}

/** Choisit un fichier puis le joint ; faux si l'utilisateur annule. */
export async function addAttachmentFromDialog(doc: OpenDoc): Promise<boolean> {
  const path = await pickAnyFile();
  if (!path) return false;
  return applyEdit(doc, { kind: 'add_attachment', path });
}

/** Choisit un chemin puis y écrit la pièce jointe. */
export async function saveAttachmentToDialog(doc: OpenDoc, a: Attachment): Promise<boolean> {
  const path = await pickSaveFile(a.name);
  if (!path) return false;
  docsState.error = null;
  try {
    await apiSaveAttachment(doc.summary.id, a.index, path);
    showNotice(t.attachmentSaved);
    return true;
  } catch (e) {
    reportError(t.exportError, e);
    return false;
  }
}

/** Exporte des pages en images ; `null` en cas d'erreur (affichée dans le bandeau). */
export async function exportImagesTo(
  doc: OpenDoc,
  spec: ImageExport,
  dir: string,
  prefix: string,
  force: boolean,
): Promise<string[] | null> {
  docsState.error = null;
  try {
    const written = await apiExportImages(doc.summary.id, spec, dir, prefix, force);
    showNotice(t.exportedImages(written.length));
    return written;
  } catch (e) {
    reportError(t.exportError, e);
    return null;
  }
}

export async function exportTextTo(
  doc: OpenDoc,
  pages: number[] | null,
  separators: boolean,
  path: string,
): Promise<boolean> {
  docsState.error = null;
  try {
    await apiExportText(doc.summary.id, pages, separators, path);
    showNotice(t.exportedText);
    return true;
  } catch (e) {
    reportError(t.exportError, e);
    return false;
  }
}

export function setSelectedPages(doc: OpenDoc, pages: number[]): void {
  doc.selectedPages = pages;
}

export function clearError(): void {
  docsState.error = null;
}
