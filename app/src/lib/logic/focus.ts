// SPDX-License-Identifier: GPL-3.0-or-later
// Piège de focus des dialogues modaux : quel élément reçoit le focus après Tab / Maj+Tab.

/**
 * Indice du prochain élément focalisable, en bouclant, sachant lesquels sont utilisables
 * (`usable[i]` : ni désactivé ni caché) ; `current` vaut -1 si le focus est hors du dialogue.
 * Retourne -1 si rien ne peut recevoir le focus.
 */
export function nextFocus(current: number, usable: boolean[], shift: boolean): number {
  const n = usable.length;
  if (n === 0) return -1;
  const start = current < 0 ? (shift ? n : -1) : current;
  const dir = shift ? -1 : 1;
  for (let k = 1; k <= n; k++) {
    const i = (((start + dir * k) % n) + n) % n;
    if (usable[i]) return i;
  }
  return -1;
}

/** Ce qu'il faut savoir d'un élément focalisable pour les groupes de boutons radio. */
export interface RadioLike {
  /** Nom du groupe pour un bouton radio ; `null` pour tout autre élément. */
  name: string | null;
  checked: boolean;
}

/**
 * Arrêts de tabulation : un seul par groupe de boutons radio (le coché, sinon le premier),
 * comme le navigateur ; les flèches parcourent ensuite le groupe (C24, Lot 9). Tout autre
 * élément est un arrêt.
 */
export function radioTabStops(items: RadioLike[]): boolean[] {
  const checkedGroups = new Set(items.filter((i) => i.name && i.checked).map((i) => i.name));
  const seen = new Set<string>();
  return items.map((item) => {
    if (!item.name) return true;
    if (checkedGroups.has(item.name)) return item.checked;
    if (seen.has(item.name)) return false;
    seen.add(item.name);
    return true;
  });
}
