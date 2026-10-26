<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { activate, closeDoc, docsState } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';

  function labelOf(path: string | null, title: string | null): string {
    if (title) return title;
    if (path) return path.split(/[\\/]/).pop() ?? path;
    return t.untitled;
  }
</script>

{#if docsState.docs.length > 0}
  <div class="tabs" role="tablist" aria-label={t.appName}>
    {#each docsState.docs as doc (doc.summary.id)}
      {@const active = doc.summary.id === docsState.activeId}
      <div class="tab" class:active role="presentation">
        <button
          type="button"
          role="tab"
          aria-selected={active}
          title={doc.summary.path ?? ''}
          onclick={() => activate(doc.summary.id)}
          onauxclick={(e) => {
            if (e.button === 1) closeDoc(doc.summary.id);
          }}
        >
          {labelOf(doc.summary.path, doc.summary.title)}
        </button>
        <button
          type="button"
          class="close"
          aria-label={t.closeTab}
          title={t.closeTab}
          onclick={() => closeDoc(doc.summary.id)}>×</button
        >
      </div>
    {/each}
  </div>
{/if}

<style>
  .tabs {
    display: flex;
    gap: 2px;
    padding: 0.25rem 0.5rem 0;
    background: var(--panel);
    border-bottom: 1px solid var(--panel-border);
    overflow-x: auto;
  }
  .tab {
    display: flex;
    align-items: center;
    max-width: 220px;
    border: 1px solid transparent;
    border-bottom: none;
    border-radius: 6px 6px 0 0;
  }
  .tab.active {
    background: var(--bg);
    border-color: var(--panel-border);
  }
  .tab > button[role='tab'] {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 180px;
  }
  .close {
    padding: 0.1rem 0.4rem;
    line-height: 1;
  }
</style>
