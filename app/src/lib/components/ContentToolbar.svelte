<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import MenuButton from './MenuButton.svelte';
  import Icon from './Icon.svelte';
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
  const hint = $derived(
    toolState.tool === 'objects' ? t.objectHint : toolState.tool === 'image' ? t.imageTool : null,
  );
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
  <MenuButton label={t.contentMenu}>
    {#snippet items(pick)}
      <li role="none">
        <button type="button" role="menuitem" class="with-icon" onclick={() => pick(onwatermark)}
          ><span class="slot"></span>{t.watermarkTitle}…</button
        >
      </li>
      <li role="none">
        <button type="button" role="menuitem" class="with-icon" onclick={() => pick(onnumbering)}
          ><span class="slot"></span>{t.numberingTitle}…</button
        >
      </li>
      <li role="none">
        <button type="button" role="menuitem" class="with-icon" onclick={() => pick(oncrop)}
          ><span class="slot"></span>{t.cropTitle}…</button
        >
      </li>
      <li role="none">
        <button type="button" role="menuitem" class="with-icon" onclick={() => pick(onocr)}
          ><Icon name="ocr" size={18} />{t.ocrMenu}</button
        >
      </li>
    {/snippet}
  </MenuButton>
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
  /* Icône et libellé ; les entrées sans icône gardent la place pour rester alignées. */
  .with-icon {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .slot {
    width: 18px;
    flex: none;
  }
  .hint {
    color: var(--muted);
    font-size: 0.8rem;
    max-width: 22rem;
  }
</style>
