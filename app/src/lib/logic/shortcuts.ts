// SPDX-License-Identifier: GPL-3.0-or-later
// Table unique des raccourcis clavier : la même source sert à réagir aux touches et à
// générer l'aide (F1).

import { t } from '$lib/strings';

export type Action =
  | 'open'
  | 'close'
  | 'find'
  | 'findNext'
  | 'findPrev'
  | 'zoomIn'
  | 'zoomOut'
  | 'zoomReset'
  | 'fitWidth'
  | 'fitPage'
  | 'rotate'
  | 'firstPage'
  | 'lastPage'
  | 'nextPage'
  | 'prevPage'
  | 'help'
  | 'escape'
  | 'properties'
  | 'undo'
  | 'redo'
  | 'save'
  | 'saveAs'
  | 'merge'
  | 'split'
  | 'toolNote'
  | 'toolHighlight'
  | 'toolUnderline'
  | 'toolStrikeout'
  | 'toolText'
  | 'toolInk'
  | 'deleteAnnotation'
  | 'print'
  | 'preferences';

/** Forme minimale d'un événement clavier, indépendante du DOM (testable). */
export interface KeyLike {
  key: string;
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
}

interface Binding {
  key: string;
  ctrl?: boolean;
  shift?: boolean;
  action: Action;
}

const BINDINGS: Binding[] = [
  { key: 'o', ctrl: true, action: 'open' },
  { key: 'w', ctrl: true, action: 'close' },
  { key: 'f', ctrl: true, action: 'find' },
  { key: 'F3', action: 'findNext' },
  { key: 'F3', shift: true, action: 'findPrev' },
  { key: '+', ctrl: true, action: 'zoomIn' },
  { key: '=', ctrl: true, action: 'zoomIn' },
  { key: '-', ctrl: true, action: 'zoomOut' },
  { key: '0', ctrl: true, action: 'zoomReset' },
  { key: '1', ctrl: true, action: 'fitWidth' },
  { key: '2', ctrl: true, action: 'fitPage' },
  { key: 'i', ctrl: true, action: 'properties' },
  { key: 'p', ctrl: true, action: 'print' },
  { key: ',', ctrl: true, action: 'preferences' },
  { key: 'z', ctrl: true, action: 'undo' },
  { key: 'y', ctrl: true, action: 'redo' },
  { key: 'z', ctrl: true, shift: true, action: 'redo' },
  { key: 's', ctrl: true, action: 'save' },
  { key: 's', ctrl: true, shift: true, action: 'saveAs' },
  { key: 'm', ctrl: true, action: 'merge' },
  { key: 'd', ctrl: true, shift: true, action: 'split' },
  { key: 'n', action: 'toolNote' },
  { key: 'h', action: 'toolHighlight' },
  { key: 'u', action: 'toolUnderline' },
  { key: 'k', action: 'toolStrikeout' },
  { key: 't', action: 'toolText' },
  { key: 'd', action: 'toolInk' },
  { key: 'Delete', action: 'deleteAnnotation' },
  { key: 'r', action: 'rotate' },
  { key: 'Home', action: 'firstPage' },
  { key: 'End', action: 'lastPage' },
  { key: 'PageDown', action: 'nextPage' },
  { key: 'PageUp', action: 'prevPage' },
  { key: 'F1', action: 'help' },
  { key: 'Escape', action: 'escape' },
];

/** L'action associée à une touche, ou `null`. Les modificateurs doivent correspondre exactement. */
export function actionFor(event: KeyLike): Action | null {
  if (event.alt) return null;
  const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
  for (const b of BINDINGS) {
    const wantCtrl = b.ctrl ?? false;
    const wantShift = b.shift ?? false;
    if (b.key === key && wantCtrl === event.ctrl && wantShift === event.shift) return b.action;
  }
  return null;
}

function label(b: Binding): string {
  const parts: string[] = [];
  if (b.ctrl) parts.push('Ctrl');
  if (b.shift) parts.push(t.keyShift);
  parts.push(b.key.length === 1 ? b.key.toUpperCase() : (t.keyNames[b.key] ?? b.key));
  return parts.join('+');
}

/**
 * Aide des raccourcis : une ligne par action, toutes ses touches, un libellé dans la langue
 * **courante** (calculée à l'appel, pas une fois pour toutes à l'import du module).
 */
export function shortcutHelp(): { action: Action; keys: string[]; label: string }[] {
  const byAction = new Map<Action, string[]>();
  for (const b of BINDINGS) {
    const keys = byAction.get(b.action) ?? [];
    keys.push(label(b));
    byAction.set(b.action, keys);
  }
  return [...byAction.entries()].map(([action, keys]) => ({
    action,
    keys,
    label: t.actions[action],
  }));
}
