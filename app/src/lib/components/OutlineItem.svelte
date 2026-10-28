<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import type { OutlineNode } from '$lib/api';
  import { goToPage, type OpenDoc } from '$lib/state/documents.svelte';
  import OutlineItem from './OutlineItem.svelte';

  let { doc, node, depth }: { doc: OpenDoc; node: OutlineNode; depth: number } = $props();
  // Déplié ou replié : gardé dans la vue du document (survit au changement de langue).
  const key = $derived(`${depth}:${node.page ?? ''}:${node.title}`);
  const open = $derived(doc.view.outlineOpen[key] ?? depth < 1);
</script>

<li
  role="treeitem"
  aria-selected={false}
  aria-expanded={node.children.length > 0 ? open : undefined}
>
  <div class="row" style:padding-left="{0.5 + depth * 0.9}rem">
    {#if node.children.length > 0}
      <button
        type="button"
        class="toggle"
        aria-label={open ? '−' : '+'}
        onclick={() => (doc.view.outlineOpen[key] = !open)}>{open ? '▾' : '▸'}</button
      >
    {:else}
      <span class="toggle"></span>
    {/if}
    <button
      type="button"
      class="label"
      disabled={node.page === null}
      onclick={() => node.page !== null && goToPage(doc, node.page)}
    >
      {node.title}
    </button>
  </div>
  {#if open && node.children.length > 0}
    <ul role="group">
      {#each node.children as child, i (i)}
        <OutlineItem {doc} node={child} depth={depth + 1} />
      {/each}
    </ul>
  {/if}
</li>

<style>
  li {
    list-style: none;
  }
  ul {
    margin: 0;
    padding: 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .toggle {
    width: 1.4rem;
    padding: 0.1rem;
    text-align: center;
    flex: none;
  }
  .label {
    flex: 1;
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
