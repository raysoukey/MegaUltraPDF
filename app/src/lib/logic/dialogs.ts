// SPDX-License-Identifier: GPL-3.0-or-later
// Quel dialogue de la page est ouvert (logique pure, testée) : un seul à la fois.

/** Les dialogues ouverts depuis les menus et les raccourcis de la page principale. */
export type Dialog =
  | 'properties'
  | 'help'
  | 'preferences'
  | 'merge'
  | 'split'
  | 'watermark'
  | 'ocr'
  | 'sign'
  | 'numbering'
  | 'crop'
  | 'export'
  | 'print'
  | 'protect'
  | 'optimize'
  | 'compare';

/** Dialogues utiles sans document ouvert ; tous les autres agissent sur le document actif. */
export const NO_DOC: ReadonlySet<Dialog> = new Set<Dialog>(['help', 'preferences', 'merge']);

/** Vrai si `dialog` peut s'ouvrir (un document est-il ouvert ?). */
export function canOpen(dialog: Dialog, hasDoc: boolean): boolean {
  return hasDoc || NO_DOC.has(dialog);
}

/**
 * Le dialogue ouvert après la demande d'ouvrir `wanted` : un seul à la fois (une demande
 * pendant qu'un autre est ouvert est ignorée), et jamais un dialogue de document sans
 * document.
 */
export function request(current: Dialog | null, wanted: Dialog, hasDoc: boolean): Dialog | null {
  if (current !== null) return current;
  return canOpen(wanted, hasDoc) ? wanted : null;
}

/** Échap ferme le dialogue, sauf pendant une opération (reconnaissance, signature). */
export function afterEscape(current: Dialog | null, busy: boolean): Dialog | null {
  return busy ? current : null;
}

/** Vrai si `dialog` est celui à afficher (un dialogue de document disparaît avec lui). */
export function isShown(dialog: Dialog, current: Dialog | null, hasDoc: boolean): boolean {
  return current === dialog && canOpen(dialog, hasDoc);
}
