// SPDX-License-Identifier: GPL-3.0-or-later
// Enveloppes typées des commandes Tauri. Les types reflètent ceux définis en Rust
// (`crates/mega-core` et `app/src-tauri/src/commands.rs`) : si l'un change, changer l'autre.

import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';
import { parseCounters } from '$lib/logic/export';

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
  | { kind: 'update_annotation'; page: number; index: number; patch: AnnotationPatch }
  | { kind: 'move_annotation'; page: number; index: number; rect: Rect }
  | { kind: 'set_field_value'; page: number; index: number; value: FieldValue }
  | { kind: 'flatten_form' }
  | { kind: 'insert_signature'; page: number; rect: Rect; source: SignatureSource }
  | { kind: 'delete_object'; page: number; index: number }
  | { kind: 'move_object'; page: number; index: number; dx: number; dy: number }
  | { kind: 'resize_object'; page: number; index: number; rect: Rect }
  | { kind: 'set_object_text'; page: number; index: number; text: string }
  | { kind: 'insert_text'; page: number; at: [number, number]; text: string; style: TextStyle }
  | { kind: 'insert_image'; page: number; rect: Rect; path: string }
  | { kind: 'ocr_page'; page: number; languages: string[] }
  | { kind: 'redact'; page: number; rect: Rect }
  | { kind: 'watermark'; spec: Watermark }
  | { kind: 'page_numbers'; spec: PageNumbering }
  | { kind: 'crop_pages'; spec: CropSpec }
  | { kind: 'set_metadata'; patch: MetadataPatch }
  | { kind: 'protect'; protection: Protection }
  | { kind: 'unprotect'; password: string | null }
  | { kind: 'optimize'; options: OptimizeOptions }
  | { kind: 'add_attachment'; path: string }
  | { kind: 'delete_attachment'; index: number };

/** Format d'export d'images (miroir de `ImageFormat`). */
export type ImageFormat = { kind: 'png' } | { kind: 'jpeg'; quality: number };

/** Export d'images (miroir de `ImageExport`). */
export interface ImageExport {
  pages: number[] | null;
  format: ImageFormat;
  dpi: number;
  with_annotations: boolean;
}

/** Métadonnées à écrire (miroir de `MetadataPatch`) : absent = inchangé, vide = retiré. */
export interface MetadataPatch {
  title?: string;
  author?: string;
  subject?: string;
  keywords?: string;
}

/** Permissions d'un document protégé (miroir de `security::Permissions`). */
export interface Permissions {
  print: boolean;
  copy: boolean;
  modify: boolean;
  annotate: boolean;
}

/** Demande de protection (miroir de `Protection`). */
export interface Protection {
  user_password: string;
  owner_password: string;
  permissions: Permissions;
}

/** État de sécurité (miroir de `SecurityInfo`). */
export interface SecurityInfo {
  encrypted: boolean;
  user_password_required: boolean;
  permissions: Permissions;
}

/** Réglages d'optimisation (miroir de `OptimizeOptions`). */
export interface OptimizeOptions {
  image_quality: number | null;
  max_image_px: number | null;
  compress_streams: boolean;
}

/** Rapport d'optimisation (miroir de `OptimizeReport`). */
export interface OptimizeReport {
  bytes_before: number;
  bytes_after: number;
  images_recompressed: number;
  objects_removed: number;
}

/** Une pièce jointe (miroir de `Attachment`). */
export interface Attachment {
  index: number;
  name: string;
  size: number;
}

/** Résultat d'une comparaison : image PNG et compteurs. */
export interface ComparedPages {
  png: Uint8Array<ArrayBuffer>;
  differing_pixels: number;
  total_pixels: number;
  width_px: number;
  height_px: number;
}

/** Nature d'un objet de page (miroir de `ObjectKind`). */
export type ObjectKind = 'text' | 'image' | 'path' | 'form' | 'shading' | 'other';

/** Un objet du contenu d'une page, en coordonnées affichées (miroir de `PageObject`). */
export interface PageObject {
  index: number;
  kind: ObjectKind;
  bounds: Rect;
  text: string | null;
  font_size: number | null;
  font: string | null;
}

/** Famille de police : Standard 14 ou Unicode incorporée (miroir de `FontFamily`). */
export type FontFamily = 'helvetica' | 'times' | 'courier' | 'unicode';

/** Style d'un texte inséré (miroir de `TextStyle`). */
export interface TextStyle {
  family: FontFamily;
  bold: boolean;
  italic: boolean;
  size: number;
  color: Color;
}

/** Ce qu'un caviardage a retiré (miroir de `RedactReport`). */
export interface RedactReport {
  texts: number;
  images_removed: number;
  images_redacted: number;
  paths_removed: number;
  paths_kept: number;
  annotations: number;
}

/** Filigrane (miroir de `overlay::Watermark`). */
export interface Watermark {
  text: string;
  size: number;
  opacity: number;
  angle_deg: number;
  color: Color;
  pages: number[] | null;
}

/** Position d'un numéro de page (miroir de `Corner`). */
export type Corner =
  'top_left' | 'top_center' | 'top_right' | 'bottom_left' | 'bottom_center' | 'bottom_right';

/** Numérotation (miroir de `PageNumbering`) : `{n}`, `{total}`, `{titre}`. */
export interface PageNumbering {
  template: string;
  corner: Corner;
  margin: number;
  size: number;
  start: number;
  pages: number[] | null;
}

/** Recadrage par marges affichées `[gauche, bas, droite, haut]` (miroir de `CropSpec`). */
export interface CropSpec {
  pages: number[] | null;
  margins: [number, number, number, number];
}

/** Nature du formulaire (miroir de `FormType`). */
export type FormType = 'none' | 'acro_form' | 'xfa';

/** Genre de champ (miroir de `FieldKind`). */
export type FieldKind =
  'text' | 'checkbox' | 'radio' | 'combo' | 'list' | 'button' | 'signature' | 'unknown';

export interface FieldOption {
  label: string;
  selected: boolean;
}

/** Un champ de formulaire lu (miroir de `FormField`). */
export interface FormField {
  page: number;
  index: number;
  name: string;
  kind: FieldKind;
  value: string | null;
  checked: boolean | null;
  options: FieldOption[];
  rect: Rect;
  read_only: boolean;
  required: boolean;
  multiline: boolean;
  password: boolean;
}

export interface FormInfo {
  form_type: FormType;
  field_count: number;
}

/** Valeur à écrire (miroir de `FieldValue`). */
export type FieldValue = { kind: 'text'; text: string } | { kind: 'checked'; checked: boolean };

/** Source d'une signature visuelle (miroir de `SignatureSource`). */
export type SignatureSource =
  { kind: 'image'; path: string } | { kind: 'strokes'; strokes: [number, number][][] };

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

/** Modification partielle (miroir de `AnnotationPatch`) ; les champs absents ne changent pas. */
export interface AnnotationPatch {
  contents?: string;
  author?: string;
  color?: Color;
  opacity?: number;
  stroke_width?: number;
  font_size?: number;
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
  /** Déplaçable / modifiable par l'application (décidé en Rust). */
  editable: boolean;
}

/** État de l'historique (miroir de `EditState`). */
export interface EditState {
  modified: boolean;
  can_undo: boolean;
  can_redo: boolean;
  undo_label: string | null;
  redo_label: string | null;
  /** Compte rendu du caviardage que l'édition vient d'appliquer (sinon `null`). */
  last_redaction: RedactReport | null;
}

/** Résultat d'une édition (miroir de `EditResult`). */
export interface EditResult {
  summary: DocumentSummary;
  state: EditState;
}

/** État de l'OCR (miroir de `mega_core::ocr::OcrStatus`). */
export interface OcrStatus {
  available: boolean;
  path: string | null;
  languages: string[];
}

/** Raison d'un verdict (miroir de `SigIssue`) : traduite par `strings.ts` (`sigIssues`). */
export type SigIssue =
  | 'cms_unreadable'
  | 'not_signed_data'
  | 'not_detached'
  | 'multiple_signers'
  | 'no_signer'
  | 'signer_certificate_missing'
  | 'digest_algorithm'
  | 'no_signed_attributes'
  | 'message_digest_missing'
  | 'content_modified'
  | 'content_type_invalid'
  | 'signing_time_outside_validity'
  | 'signed_attributes_unreadable'
  | 'signing_certificate_mismatch'
  | 'public_key_unreadable'
  | 'weak_key'
  | 'unsupported_curve'
  | 'signature_value_unreadable'
  | 'signature_mismatch'
  | 'signature_algorithm'
  | 'unsupported_format'
  | 'byte_range_invalid'
  | 'contents_invalid'
  | 'usage_rights';

/** Verdict d'une signature (miroir de `SignatureStatus`, sérialisé `{ kind, issue }`). */
export type SignatureStatus =
  | { kind: 'intact' }
  | { kind: 'invalid'; issue: SigIssue }
  | { kind: 'unsupported'; issue: SigIssue };

/** Une signature numérique vérifiée (miroir de `mega_core::digital_signature::SignatureInfo`). */
export interface SignatureInfo {
  field: string;
  signer: string | null;
  /** Secondes Unix. */
  signed_at: number | null;
  reason: string | null;
  location: string | null;
  sub_filter: string;
  status: SignatureStatus;
  covers_whole_document: boolean;
}

/** Tampon d'une signature visible (miroir de `SignAppearance`). */
export interface SignAppearance {
  /** Page, à partir de 0. */
  page: number;
  /** Rectangle du tampon, points, espace affiché. */
  rect: Rect;
  /** Image facultative (PNG ou JPEG). */
  image_path: string | null;
}

/** Informations écrites dans une signature (miroir de `SignOptions`). */
export interface SignOptions {
  reason: string | null;
  location: string | null;
  contact: string | null;
  /** Tampon visible ; `null` : signature invisible. */
  appearance: SignAppearance | null;
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

/** Annotations de toutes les pages, une liste par page (une seule requête). */
export async function allAnnotations(id: number): Promise<Annotation[][]> {
  return invoke<Annotation[][]>('all_annotations', { id });
}

/** Champs de formulaire de toutes les pages, une liste par page (une seule requête). */
export async function allFormFields(id: number): Promise<FormField[][]> {
  return invoke<FormField[][]>('all_form_fields', { id });
}

export async function annotations(id: number, page: number): Promise<Annotation[]> {
  return invoke<Annotation[]>('annotations', { id, page });
}

export async function pageObjects(id: number, page: number): Promise<PageObject[]> {
  return invoke<PageObject[]>('page_objects', { id, page });
}

export async function formFields(id: number, page: number): Promise<FormField[]> {
  return invoke<FormField[]>('form_fields', { id, page });
}

export async function formInfo(id: number): Promise<FormInfo> {
  return invoke<FormInfo>('form_info', { id });
}

export async function pageText(id: number, page: number): Promise<string> {
  return invoke<string>('page_text', { id, page });
}

export async function exportImages(
  id: number,
  spec: ImageExport,
  dir: string,
  prefix: string,
  force: boolean,
): Promise<string[]> {
  return invoke<string[]>('export_images', { id, spec, dir, prefix, force });
}

export async function exportText(
  id: number,
  pages: number[] | null,
  separators: boolean,
  path: string,
): Promise<void> {
  await invoke('export_text', { id, pages, separators, path });
}

/** Ce que l'optimisation ferait, sans rien modifier. */
export async function optimizePreview(
  id: number,
  options: OptimizeOptions,
): Promise<OptimizeReport> {
  return invoke<OptimizeReport>('optimize_preview', { id, options });
}

export async function security(id: number): Promise<SecurityInfo> {
  return invoke<SecurityInfo>('security', { id });
}

export async function ocrStatus(): Promise<OcrStatus> {
  return invoke<OcrStatus>('ocr_status');
}

/** Fichiers reçus du Finder (macOS) en attente d'ouverture ; la liste est vidée. */
export async function takePendingPaths(): Promise<string[]> {
  return invoke<string[]>('take_pending_paths');
}

/** Chemin de Tesseract choisi dans les préférences (`null` ou vide : recherche automatique). */
export async function setTesseractPath(path: string | null): Promise<void> {
  return invoke<void>('set_tesseract_path', { path });
}

/** Choisir l'exécutable de Tesseract (dialogue natif). */
export async function pickExecutable(): Promise<string | null> {
  const picked = await open({ multiple: false, directory: false });
  return typeof picked === 'string' ? picked : null;
}

export async function pagesWithText(id: number): Promise<boolean[]> {
  return invoke<boolean[]>('pages_with_text', { id });
}

export async function signatures(id: number): Promise<SignatureInfo[]> {
  return invoke<SignatureInfo[]>('signatures', { id });
}

export async function signDocument(
  id: number,
  path: string,
  p12: string,
  password: string,
  options: SignOptions,
): Promise<EditResult> {
  return invoke<EditResult>('sign_document', { id, path, p12, password, options });
}

export async function attachments(id: number): Promise<Attachment[]> {
  return invoke<Attachment[]>('attachments', { id });
}

export async function saveAttachment(id: number, index: number, path: string): Promise<void> {
  await invoke('save_attachment', { id, index, path });
}

/**
 * Compare deux pages. Le backend renvoie un tampon binaire : 24 octets de compteurs
 * (deux entiers 64 bits, deux entiers 32 bits, petit-boutiste) puis l'image PNG.
 */
export async function comparePages(
  idA: number,
  pageA: number,
  idB: number,
  pageB: number,
  scale: number,
): Promise<ComparedPages> {
  const raw = await invoke<ArrayBuffer>('compare_pages', { idA, pageA, idB, pageB, scale });
  return parseCounters(raw);
}

/** Dialogue natif : n'importe quel fichier ; `null` si annulé. */
export async function pickAnyFile(): Promise<string | null> {
  const picked = await open({ multiple: false, directory: false });
  return typeof picked === 'string' ? picked : null;
}

/** Dialogue natif d'enregistrement pour un fichier quelconque (texte, pièce jointe). */
export async function pickSaveFile(defaultPath?: string): Promise<string | null> {
  const picked = await save({ defaultPath });
  return typeof picked === 'string' ? picked : null;
}

/** Dialogue natif : une image PNG ou JPEG ; `null` si annulé. */
export async function pickImageFile(): Promise<string | null> {
  const picked = await open({
    multiple: false,
    directory: false,
    filters: [{ name: 'Image', extensions: ['png', 'jpg', 'jpeg'] }],
  });
  return typeof picked === 'string' ? picked : null;
}

/** Dialogue natif : un certificat PKCS#12 (.p12, .pfx) ; `null` si annulé. */
export async function pickCertificateFile(): Promise<string | null> {
  const picked = await open({
    multiple: false,
    directory: false,
    filters: [{ name: 'PKCS#12', extensions: ['p12', 'pfx'] }],
  });
  return typeof picked === 'string' ? picked : null;
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
