<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import {
    addAttachmentFromDialog,
    deleteAttachment,
    ensureAttachments,
    saveAttachmentToDialog,
    type OpenDoc,
  } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';

  let { doc }: { doc: OpenDoc } = $props();

  $effect(() => {
    void ensureAttachments(doc);
  });
  const list = $derived(doc.attachments ?? []);
</script>

<div class="panel attachments" aria-label={t.attachmentsTab}>
  <p class="summary">{t.attachmentCount(list.length)}</p>
  {#if list.length === 0}
    <p class="muted">{t.noAttachments}</p>
  {:else}
    <ul role="list">
      {#each list as a (a.index)}
        <li>
          <span class="name" title={a.name}>{a.name}</span>
          <span class="muted">{t.bytes(a.size)}</span>
          <span class="buttons">
            <button
              type="button"
              aria-label={t.attachmentExtract}
              title={t.attachmentExtract}
              onclick={() => saveAttachmentToDialog(doc, a)}>⤓</button
            >
            <button
              type="button"
              aria-label={t.attachmentDelete}
              title={t.attachmentDelete}
              onclick={() => deleteAttachment(doc, a.index)}>🗑</button
            >
          </span>
        </li>
      {/each}
    </ul>
  {/if}
  <button type="button" onclick={() => addAttachmentFromDialog(doc)}>{t.attachmentAdd}</button>
</div>

<style>
  .panel {
    padding: 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .summary {
    margin: 0;
    font-weight: 500;
  }
  .muted {
    color: var(--muted);
    font-size: 0.85rem;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }
  li {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 0.1rem 0.5rem;
    align-items: center;
    padding: 0.3rem 0.4rem;
    border: 1px solid var(--panel-border);
    border-radius: 6px;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .buttons {
    grid-row: 1 / span 2;
    display: flex;
    gap: 0.2rem;
  }
</style>
