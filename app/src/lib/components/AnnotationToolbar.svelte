<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import type { AnnotationKind } from '$lib/api';
  import type { Tool } from '$lib/logic/tools';
  import {
    annotateSelection,
    deleteSelectedAnnotation,
    showNotice,
    type OpenDoc,
  } from '$lib/state/documents.svelte';
  import { saveSettings, settings } from '$lib/state/settings.svelte';
  import { currentStyle, setStamp, setTool, toolState } from '$lib/state/tools.svelte';
  import { STAMP_IDS, stampLabel, type StampId } from '$lib/logic/tools';
  import { t } from '$lib/strings';
  import ContentToolbar from './ContentToolbar.svelte';
  import StylePicker from './StylePicker.svelte';

  let {
    doc,
    onedit,
    onwatermark,
    onnumbering,
    oncrop,
    onocr,
  }: {
    doc: OpenDoc;
    onedit: () => void;
    onwatermark: () => void;
    onnumbering: () => void;
    oncrop: () => void;
    onocr: () => void;
  } = $props();

  const TOOLS: { tool: Tool; glyph: string; key: string }[] = [
    { tool: 'select', glyph: '⬚', key: 'Escape' },
    { tool: 'note', glyph: '🗨', key: 'N' },
    { tool: 'text', glyph: 'T', key: 'T' },
    { tool: 'ink', glyph: '✎', key: 'D' },
    { tool: 'rect', glyph: '▭', key: '' },
    { tool: 'ellipse', glyph: '◯', key: '' },
    { tool: 'line', glyph: '╱', key: '' },
    { tool: 'arrow', glyph: '➚', key: '' },
    { tool: 'stamp', glyph: '✔', key: '' },
    { tool: 'signature', glyph: '✍', key: '' },
  ];
  const MARKUPS: { tool: Tool; kind: AnnotationKind; glyph: string; key: string }[] = [
    { tool: 'highlight', kind: 'highlight', glyph: '🖍', key: 'H' },
    { tool: 'underline', kind: 'underline', glyph: 'U̲', key: 'U' },
    { tool: 'strikeout', kind: 'strike_out', glyph: 'S̶', key: 'K' },
  ];

  async function markup(tool: Tool, kind: AnnotationKind) {
    setTool(tool);
    const ok = await annotateSelection(doc, kind, currentStyle(tool), settings.author);
    if (!ok) showNotice(t.selectTextFirst);
    setTool('select');
  }
</script>

<div class="annotation-toolbar" role="toolbar" aria-label={t.annotationTools}>
  <div class="group">
    {#each TOOLS as item (item.tool)}
      <button
        type="button"
        class:active={toolState.tool === item.tool}
        aria-pressed={toolState.tool === item.tool}
        aria-label={t.tool[item.tool]}
        title="{t.tool[item.tool]}{item.key ? ` (${t.keyNames[item.key] ?? item.key})` : ''}"
        onclick={() => setTool(item.tool)}>{item.glyph}</button
      >
    {/each}
    {#if toolState.tool === 'stamp'}
      <select
        aria-label={t.stampLabel}
        value={toolState.stamp}
        onchange={(e) => setStamp((e.currentTarget as HTMLSelectElement).value as StampId)}
      >
        {#each STAMP_IDS as s (s)}
          <option value={s}>{stampLabel(s)}</option>
        {/each}
      </select>
    {/if}
  </div>
  <div class="group">
    {#each MARKUPS as m (m.tool)}
      <button
        type="button"
        disabled={!doc.selection}
        aria-label={t.tool[m.tool]}
        title="{t.tool[m.tool]} ({m.key})"
        onclick={() => markup(m.tool, m.kind)}>{m.glyph}</button
      >
    {/each}
  </div>
  <div class="group">
    <StylePicker />
  </div>
  <ContentToolbar {doc} {onwatermark} {onnumbering} {oncrop} {onocr} />
  <div class="group right">
    <button
      type="button"
      disabled={!doc.selectedAnnotation}
      aria-label={t.editAnnotation}
      title={t.editAnnotation}
      onclick={onedit}>✎</button
    >
    <button
      type="button"
      disabled={!doc.selectedAnnotation}
      aria-label={t.deleteAnnotation}
      title="{t.deleteAnnotation} ({t.keyNames.Delete})"
      onclick={() => deleteSelectedAnnotation(doc)}>🗑</button
    >
    <input
      type="text"
      class="author"
      aria-label={t.author}
      placeholder={t.authorPlaceholder}
      bind:value={settings.author}
      onchange={() => saveSettings()}
    />
  </div>
</div>

<style>
  .annotation-toolbar {
    display: flex;
    align-items: center;
    gap: 0.9rem;
    padding: 0.25rem 0.75rem;
    background: var(--panel);
    border-bottom: 1px solid var(--panel-border);
    flex-wrap: wrap;
  }
  .group {
    display: flex;
    align-items: center;
    gap: 0.2rem;
  }
  .right {
    margin-left: auto;
  }
  button.active {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .author {
    width: 10rem;
  }
</style>
