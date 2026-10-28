<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  // Bouton de menu déroulant partagé (motif ARIA « bouton de menu ») : ↓ ouvre sur le premier
  // élément, ↑ sur le dernier ; `menuNav` gère les flèches, Échap et la recherche par
  // initiale. Les éléments du menu viennent du parent (snippet `items`), qui reçoit `pick` :
  // fermer le menu, rendre le focus au bouton, puis lancer l'action.
  import type { Snippet } from 'svelte';
  import { menuNav } from '$lib/actions/menuNav';

  let {
    label,
    title,
    minWidth = '220px',
    items,
  }: {
    label: string;
    title?: string;
    minWidth?: string;
    items: Snippet<[(action: () => void) => void]>;
  } = $props();

  let open = $state(false);
  let openFrom = $state<'first' | 'last'>('first');
  let trigger: HTMLButtonElement | undefined = $state();

  function close(refocus: boolean) {
    open = false;
    if (refocus) trigger?.focus();
  }

  function pick(action: () => void) {
    // Le focus revient au bouton **avant** l'action : c'est lui que le piège de focus d'un
    // dialogue ouvert par l'action retiendra comme origine (l'élément du menu va disparaître).
    close(true);
    action();
  }
</script>

<div class="menu">
  <button
    type="button"
    aria-haspopup="menu"
    aria-expanded={open}
    {title}
    bind:this={trigger}
    onkeydown={(e) => {
      if ((e.key === 'ArrowDown' || e.key === 'ArrowUp') && !open) {
        e.preventDefault();
        openFrom = e.key === 'ArrowUp' ? 'last' : 'first';
        open = true;
      }
    }}
    onclick={() => {
      openFrom = 'first';
      open = !open;
    }}>{label} <span aria-hidden="true">▾</span></button
  >
  {#if open}
    <ul
      class="dropdown"
      role="menu"
      aria-label={label}
      style:min-width={minWidth}
      use:menuNav={{ onclose: close, start: openFrom }}
    >
      {@render items(pick)}
    </ul>
  {/if}
</div>

<style>
  .menu {
    position: relative;
  }
  .dropdown {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 20;
    list-style: none;
    margin: 0;
    padding: 0.25rem;
    background: var(--panel);
    border: 1px solid var(--panel-border);
    border-radius: 8px;
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.25);
  }
  .dropdown :global(li button) {
    width: 100%;
    text-align: left;
    white-space: nowrap;
  }
</style>
