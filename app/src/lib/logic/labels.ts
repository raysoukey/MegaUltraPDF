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
