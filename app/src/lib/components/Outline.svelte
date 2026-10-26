<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { ensureOutline, type OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';
  import OutlineItem from './OutlineItem.svelte';

  let { doc }: { doc: OpenDoc } = $props();

  $effect(() => {
    ensureOutline(doc);
  });
</script>

{#if doc.outline === null}
  <p class="muted">{t.loading}</p>
{:else if doc.outline.length === 0}
  <p class="muted">{t.noBookmarks}</p>
{:else}
  <ul class="root" role="tree" aria-label={t.bookmarks}>
    {#each doc.outline as node, i (i)}
      <OutlineItem {doc} {node} depth={0} />
    {/each}
  </ul>
{/if}

<style>
  .muted {
    color: var(--muted);
    padding: 0.75rem;
    margin: 0;
  }
  .root {
    list-style: none;
    margin: 0;
    padding: 0.5rem 0;
  }
</style>
