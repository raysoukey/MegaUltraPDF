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
