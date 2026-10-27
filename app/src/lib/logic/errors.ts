// SPDX-License-Identifier: GPL-3.0-or-later
// Message d'erreur d'ouverture dans la langue de l'interface.
import type { AppError } from '$lib/api';
import { t } from '$lib/strings';

/**
 * Texte affiché quand un document ne s'ouvre pas : les erreurs connues (fichier illisible,
 * PDF invalide) sont traduites d'après leur `kind` ; les autres gardent le message détaillé
 * du cœur Rust, qui reste en français (limite documentée, ADR-0012).
 */
export function openErrorMessage(e: Pick<AppError, 'kind' | 'message'>): string {
  const known = e.kind === 'io' || e.kind === 'invalid_pdf' ? t.errorKinds[e.kind] : null;
  return t.openError(known ?? e.message);
}
