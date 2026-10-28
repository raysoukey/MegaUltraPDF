// SPDX-License-Identifier: GPL-3.0-or-later
// Action Svelte : garde le focus clavier à l'intérieur d'un dialogue modal.
//
// [Apprentissage] Une « action » Svelte (`use:trapFocus`) est une fonction appelée avec
// l'élément DOM une fois monté ; l'objet `{ destroy }` qu'elle retourne est appelé au
// démontage. La décision (quel élément suit) est la fonction pure `nextFocus`, testée
// sous Vitest ; ce fichier ne fait que la relier au DOM.
import { nextFocus, radioTabStops } from '$lib/logic/focus';

const FOCUSABLE =
  'a[href], button, input:not([type="hidden"]), select, textarea, summary, [tabindex]:not([tabindex="-1"])';

/**
 * Un élément peut-il recevoir le focus par Tab maintenant ? Ni désactivé, ni caché, ni
 * sorti de l'ordre de tabulation (`tabindex="-1"`, ex. les onglets inactifs de l'aide).
 */
function usable(el: HTMLElement): boolean {
  // `:disabled` voit aussi un champ désactivé par son `<fieldset disabled>` (C24).
  if (el.matches(':disabled') || el.tabIndex < 0) return false;
  if (el.closest('[hidden], [inert]')) return false;
  return el.getClientRects().length > 0;
}

/**
 * Piège de focus : Tab / Maj+Tab bouclent sur les éléments utilisables du dialogue, le
 * premier reçoit le focus à l'ouverture (sauf si le composant a déjà placé le focus
 * dedans), et le focus revient à l'élément d'origine à la fermeture.
 */
export function trapFocus(node: HTMLElement) {
  const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  // Le dialogue lui-même peut recevoir le focus (sans entrer dans l'ordre de tabulation) : un
  // clic sur du texte du dialogue y laisse le focus au lieu de le renvoyer au document, et
  // Tab / Maj+Tab restent donc piégés.
  if (!node.hasAttribute('tabindex')) node.tabIndex = -1;

  function elements(): HTMLElement[] {
    return Array.from(node.querySelectorAll<HTMLElement>(FOCUSABLE));
  }

  /** Utilisables, avec un seul arrêt par groupe de boutons radio. */
  function usableList(list: HTMLElement[]): boolean[] {
    return radioTabStops(
      list.map((el) =>
        el instanceof HTMLInputElement && el.type === 'radio'
          ? { name: el.name || null, checked: el.checked, usable: usable(el) }
          : { name: null, checked: false, usable: usable(el) },
      ),
    );
  }

  // Après le montage complet : un composant qui focalise son premier champ dans `onMount`
  // a priorité.
  queueMicrotask(() => {
    if (node.contains(document.activeElement)) return;
    const list = elements();
    const i = nextFocus(-1, usableList(list), false);
    if (i >= 0) list[i].focus();
    else node.focus();
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== 'Tab') return;
    const list = elements();
    const current = list.indexOf(document.activeElement as HTMLElement);
    // Depuis un bouton radio non retenu (atteint aux flèches), on part de son groupe.
    const i = nextFocus(current, usableList(list), e.shiftKey);
    e.preventDefault();
    if (i >= 0) list[i].focus();
  }

  node.addEventListener('keydown', onkeydown);
  return {
    destroy() {
      node.removeEventListener('keydown', onkeydown);
      if (previous && previous.isConnected) previous.focus();
    },
  };
}
