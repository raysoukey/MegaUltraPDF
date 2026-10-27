// SPDX-License-Identifier: GPL-3.0-or-later
// Action Svelte : navigation au clavier dans un menu déroulant ouvert (`<ul role="menu">`).
//
// La décision (quelle touche fait quoi) est la fonction pure `menuKey`, testée sous Vitest ;
// cette action la relie au DOM : focus du premier élément à l'ouverture, flèches, Échap,
// clic à l'extérieur du menu.
import { menuKey } from '$lib/logic/menu';

/** Paramètres de `menuNav` : comment fermer le menu (et faut-il rendre le focus au bouton). */
export type MenuNavOptions = { onclose: (refocus: boolean) => void };

/**
 * Rend un menu ouvert utilisable au clavier : ↓ ↑ Début Fin déplacent le focus entre les
 * éléments `role="menuitem"` actifs, Échap et Tab ferment en rendant le focus au bouton,
 * Entrée et Espace activent l'élément (clic natif du bouton), un clic hors du conteneur
 * du menu (le parent du `<ul>`, qui contient aussi le bouton) ferme sans voler le focus.
 */
export function menuNav(node: HTMLElement, options: MenuNavOptions) {
  let opts = options;
  const root = node.parentElement ?? node;

  function items(): HTMLElement[] {
    return Array.from(node.querySelectorAll<HTMLElement>('[role="menuitem"]'));
  }
  function enabled(list: HTMLElement[]): boolean[] {
    return list.map((el) => !(el as HTMLButtonElement).disabled);
  }

  // À l'ouverture, le focus va au premier élément actif (motif ARIA « bouton de menu »).
  queueMicrotask(() => {
    const list = items();
    const move = menuKey('Home', -1, enabled(list));
    if (move && 'index' in move) list[move.index].focus();
  });

  function onkeydown(e: KeyboardEvent) {
    const list = items();
    const move = menuKey(e.key, list.indexOf(document.activeElement as HTMLElement), enabled(list));
    if (!move || 'activate' in move) return;
    e.preventDefault();
    // Échap ferme le menu seulement : ni le dialogue ni la recherche en dessous.
    e.stopPropagation();
    if ('index' in move) list[move.index].focus();
    else opts.onclose(true);
  }

  function onpointerdown(e: PointerEvent) {
    if (!root.contains(e.target as Node)) opts.onclose(false);
  }

  node.addEventListener('keydown', onkeydown);
  document.addEventListener('pointerdown', onpointerdown, true);
  return {
    update(next: MenuNavOptions) {
      opts = next;
    },
    destroy() {
      node.removeEventListener('keydown', onkeydown);
      document.removeEventListener('pointerdown', onpointerdown, true);
    },
  };
}
