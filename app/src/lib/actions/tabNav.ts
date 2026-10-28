// SPDX-License-Identifier: GPL-3.0-or-later
// Action Svelte : liste d'onglets utilisable au clavier (`<div role="tablist" use:tabNav>`).
//
// Focus « itinérant » (roving tabindex) : seul l'onglet sélectionné est dans l'ordre de
// tabulation ; ← → Début Fin déplacent le focus **et** activent l'onglet visé (clic natif).
// La décision est la fonction pure `tabKey`, testée sous Vitest (C28, Lot 9).
import { tabKey } from '$lib/logic/tabs';

export function tabNav(node: HTMLElement) {
  function tabs(): HTMLElement[] {
    return Array.from(node.querySelectorAll<HTMLElement>('[role="tab"]'));
  }

  /** Seul l'onglet sélectionné reçoit Tab ; les autres s'atteignent aux flèches. */
  function rove() {
    const list = tabs();
    const selected = list.findIndex((t) => t.getAttribute('aria-selected') === 'true');
    list.forEach((t, i) => (t.tabIndex = i === Math.max(0, selected) ? 0 : -1));
  }

  function onkeydown(e: KeyboardEvent) {
    const list = tabs();
    const current = list.indexOf(document.activeElement as HTMLElement);
    if (current < 0) return;
    const next = tabKey(e.key, current, list.length);
    if (next === null) return;
    e.preventDefault();
    // Début / Fin sont aussi des raccourcis de page de la visionneuse : la touche s'arrête ici.
    e.stopPropagation();
    list[next].focus();
    list[next].click();
  }

  rove();
  // L'onglet sélectionné change (clic, clavier, programme) : on suit.
  const observer = new MutationObserver(rove);
  // Un onglet ajouté plus tard (« Formulaire » après lecture du formulaire) est aussi suivi.
  observer.observe(node, {
    subtree: true,
    childList: true,
    attributes: true,
    attributeFilter: ['aria-selected'],
  });
  node.addEventListener('keydown', onkeydown);
  return {
    destroy() {
      observer.disconnect();
      node.removeEventListener('keydown', onkeydown);
    },
  };
}
