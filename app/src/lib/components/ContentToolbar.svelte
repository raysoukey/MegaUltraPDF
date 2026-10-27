<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { menuNav } from '$lib/actions/menuNav';
  import type { Tool } from '$lib/logic/tools';
  import { deleteSelectedObject, type OpenDoc } from '$lib/state/documents.svelte';
  import { setTool, toolState } from '$lib/state/tools.svelte';
  import { t } from '$lib/strings';

  let {
    doc,
    onwatermark,
    onnumbering,
    oncrop,
    onocr,
  }: {
    doc: OpenDoc;
    onwatermark: () => void;
    onnumbering: () => void;
    oncrop: () => void;
    onocr: () => void;
  } = $props();

  const TOOLS: { tool: Tool; glyph: string }[] = [
    { tool: 'objects', glyph: '⬚' },
    { tool: 'text-content', glyph: 'A' },
    { tool: 'image', glyph: '🖼' },
    { tool: 'redact', glyph: '▬' },
  ];
  let menuOpen = $state(false);
  let openFrom = $state<'first' | 'last'>('first');
  let trigger: HTMLButtonElement | undefined = $state();

  function closeMenu(refocus: boolean) {
    menuOpen = false;
    if (refocus) trigger?.focus();
  }
  const hint = $derived(
    toolState.tool === 'objects' ? t.objectHint : toolState.tool === 'image' ? t.imageTool : null,
  );

  function pick(action: () => void) {
    // Focus rendu au bouton avant l'ouverture du dialogue (origine retenue par trapFocus).
    closeMenu(true);
    action();
  }
</script>

<div class="group" role="group" aria-label={t.contentTools}>
  {#each TOOLS as item (item.tool)}
    <button
      type="button"
      class:active={toolState.tool === item.tool}
      aria-pressed={toolState.tool === item.tool}
      aria-label={t.tool[item.tool]}
      title={t.tool[item.tool]}
      onclick={() => setTool(item.tool)}>{item.glyph}</button
    >
  {/each}
  <button
    type="button"
    disabled={!doc.selectedObject}
    aria-label={t.deleteObject}
    title="{t.deleteObject} ({t.keyNames.Delete})"
    onclick={() => deleteSelectedObject(doc)}>🗑</button
  >
  {#if hint}
    <span class="hint" role="status">{hint}</span>
  {/if}
  <div class="menu">
    <button
      type="button"
      aria-haspopup="menu"
      aria-expanded={menuOpen}
      bind:this={trigger}
      onkeydown={(e) => {
        // ↓ ouvre sur le premier élément, ↑ sur le dernier (motif ARIA « bouton de menu »).
        if ((e.key === 'ArrowDown' || e.key === 'ArrowUp') && !menuOpen) {
          e.preventDefault();
          openFrom = e.key === 'ArrowUp' ? 'last' : 'first';
          menuOpen = true;
        }
      }}
      onclick={() => {
        openFrom = 'first';
        menuOpen = !menuOpen;
      }}>{t.contentMenu} <span aria-hidden="true">▾</span></button
    >
    {#if menuOpen}
      <ul
        class="dropdown"
        role="menu"
        aria-label={t.contentMenu}
        use:menuNav={{ onclose: closeMenu, start: openFrom }}
      >
        <li role="none">
          <button type="button" role="menuitem" onclick={() => pick(onwatermark)}
            >{t.watermarkTitle}…</button
          >
        </li>
        <li role="none">
          <button type="button" role="menuitem" onclick={() => pick(onnumbering)}
            >{t.numberingTitle}…</button
          >
        </li>
        <li role="none">
          <button type="button" role="menuitem" onclick={() => pick(oncrop)}>{t.cropTitle}…</button>
        </li>
        <li role="none">
          <button type="button" role="menuitem" onclick={() => pick(onocr)}>{t.ocrMenu}</button>
        </li>
      </ul>
    {/if}
  </div>
</div>

<style>
  .group {
    display: flex;
    align-items: center;
    gap: 0.2rem;
    padding-left: 0.6rem;
    border-left: 1px solid var(--panel-border);
  }
  button.active {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .hint {
    color: var(--muted);
    font-size: 0.8rem;
    max-width: 22rem;
  }
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
    min-width: 220px;
    background: var(--panel);
    border: 1px solid var(--panel-border);
    border-radius: 8px;
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.25);
  }
  .dropdown li button {
    width: 100%;
    text-align: left;
    white-space: nowrap;
  }
</style>
