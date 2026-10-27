<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import type { Annotation } from '$lib/api';
  import type { OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';
  import AnnotationPanel from './AnnotationPanel.svelte';
  import Outline from './Outline.svelte';
  import Thumbnails from './Thumbnails.svelte';

  let {
    doc,
    oneditannotation,
  }: { doc: OpenDoc; oneditannotation: (a: Annotation, page: number) => void } = $props();
  let tab = $state<'thumbnails' | 'bookmarks' | 'annotations'>('thumbnails');
</script>

<aside class="sidebar" aria-label={t.sidebar}>
  <div class="tabs" role="tablist">
    <button
      type="button"
      role="tab"
      aria-selected={tab === 'thumbnails'}
      class:active={tab === 'thumbnails'}
      onclick={() => (tab = 'thumbnails')}>{t.thumbnails}</button
    >
    <button
      type="button"
      role="tab"
      aria-selected={tab === 'bookmarks'}
      class:active={tab === 'bookmarks'}
      onclick={() => (tab = 'bookmarks')}>{t.bookmarks}</button
    >
    <button
      type="button"
      role="tab"
      aria-selected={tab === 'annotations'}
      class:active={tab === 'annotations'}
      onclick={() => (tab = 'annotations')}>{t.annotations}</button
    >
  </div>
  <div class="content">
    {#if tab === 'thumbnails'}
      <Thumbnails {doc} />
    {:else if tab === 'bookmarks'}
      <Outline {doc} />
    {:else}
      <AnnotationPanel {doc} onedit={oneditannotation} />
    {/if}
  </div>
</aside>

<style>
  .sidebar {
    width: 220px;
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border-right: 1px solid var(--panel-border);
  }
  .tabs {
    display: flex;
    border-bottom: 1px solid var(--panel-border);
  }
  .tabs button {
    flex: 1;
    border-radius: 0;
  }
  .content {
    flex: 1;
    overflow: auto;
  }
</style>
