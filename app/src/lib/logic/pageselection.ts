// SPDX-License-Identifier: GPL-3.0-or-later
// Sélection multiple de pages (clic, Ctrl, Maj) et calcul d'un déplacement par glisser-déposer.
// Logique pure, sans DOM : les composants ne font que l'appeler.

export interface ClickMods {
  ctrl: boolean;
  shift: boolean;
}

export interface SelectionResult {
  /** Pages sélectionnées, triées, sans doublon. */
  selected: number[];
  /** Point de départ du prochain Maj+clic. */
  anchor: number;
}

/**
 * Nouvelle sélection après un clic sur `index` :
 * clic simple → cette page seule ; Ctrl → bascule ; Maj → plage depuis l'ancre.
 */
export function clickSelection(
  current: number[],
  anchor: number | null,
  index: number,
  mods: ClickMods,
): SelectionResult {
  if (mods.shift && anchor !== null) {
    const [from, to] = anchor <= index ? [anchor, index] : [index, anchor];
    const selected: number[] = [];
    for (let i = from; i <= to; i++) selected.push(i);
    return { selected, anchor };
  }
  if (mods.ctrl) {
    const set = new Set(current);
    if (set.has(index)) set.delete(index);
    else set.add(index);
    return { selected: [...set].sort((a, b) => a - b), anchor: index };
  }
  return { selected: [index], anchor: index };
}

/**
 * Ordre des pages (index d'origine) après avoir déplacé `pages` devant la page `slot`
 * (`slot === pageCount` : à la fin). Les pages déplacées restent dans l'ordre du document.
 */
export function reorder(pageCount: number, pages: number[], slot: number): number[] {
  const moving = new Set(pages);
  const block = [...moving].sort((a, b) => a - b);
  const result: number[] = [];
  for (let i = 0; i <= pageCount; i++) {
    if (i === slot) result.push(...block);
    if (i < pageCount && !moving.has(i)) result.push(i);
  }
  return result;
}

/**
 * Index d'insertion attendu par `move_pages` (compté après retrait des pages déplacées),
 * ou `null` si le dépôt ne changerait rien.
 */
export function moveTarget(pageCount: number, pages: number[], slot: number): number | null {
  if (pages.length === 0) return null;
  const order = reorder(pageCount, pages, slot);
  if (order.every((p, i) => p === i)) return null;
  const removedBefore = pages.filter((p) => p < slot).length;
  return slot - removedBefore;
}

/** Sélection après insertion de `n` pages à l'index `at` : les index ≥ `at` reculent de `n`. */
export function shiftAfterInsert(selected: number[], at: number, n: number): number[] {
  return selected.map((p) => (p >= at ? p + n : p));
}
