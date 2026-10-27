// SPDX-License-Identifier: GPL-3.0-or-later
// Libellés dérivés d'un document : nom de fichier, titre d'onglet avec marque « modifié ».

import { t } from '$lib/strings';

/** Dernier segment d'un chemin Windows ou POSIX. */
export function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** Titre d'un onglet : titre du PDF, sinon nom du fichier, sinon « (sans titre) » ; « • » si modifié. */
export function tabLabel(doc: {
  path: string | null;
  title: string | null;
  modified: boolean;
}): string {
  const base = doc.title || (doc.path ? fileName(doc.path) : t.untitled);
  return doc.modified ? `${base} ${t.modifiedMarker}` : base;
}

/** `D:20261026143000+00'00'` → `2026-10-26 14:30` ; une valeur non reconnue est rendue telle quelle. */
export function formatPdfDate(raw: string | null): string | null {
  if (!raw) return null;
  const m = /^D:(\d{4})(\d{2})?(\d{2})?(\d{2})?(\d{2})?/.exec(raw);
  if (!m) return raw;
  const [, y, mo = '01', d = '01', h = '00', mi = '00'] = m;
  return `${y}-${mo}-${d} ${h}:${mi}`;
}
