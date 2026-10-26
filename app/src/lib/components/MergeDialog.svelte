<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { pickPdfFiles } from '$lib/api';
  import { fileName } from '$lib/logic/labels';
  import { mergeAndOpen } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';

  let { initial = [], onclose }: { initial?: string[]; onclose: () => void } = $props();

  // svelte-ignore state_referenced_locally
  let files = $state<string[]>([...initial]);
  let busy = $state(false);

  async function add() {
    const picked = await pickPdfFiles();
    files = [...files, ...picked.filter((p) => !files.includes(p))];
  }
  function move(i: number, dir: -1 | 1) {
    const j = i + dir;
    if (j < 0 || j >= files.length) return;
    const next = [...files];
    [next[i], next[j]] = [next[j], next[i]];
    files = next;
  }
  function remove(i: number) {
    files = files.filter((_, k) => k !== i);
  }
  async function merge() {
    busy = true;
    const ok = await mergeAndOpen(files);
    busy = false;
    if (ok) onclose();
  }
</script>

<div
  class="dialog-backdrop"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && onclose()}
>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="merge-title">
    <h2 id="merge-title">{t.mergeTitle}</h2>
    <p class="muted">{t.mergeHint}</p>
    {#if files.length === 0}
      <p class="muted">{t.mergeEmpty}</p>
    {:else}
      <ol class="files">
        {#each files as path, i (path)}
          <li>
            <span class="name" title={path}>{fileName(path)}</span>
            <button type="button" title={t.moveUp} disabled={i === 0} onclick={() => move(i, -1)}
              >▲</button
            >
            <button
              type="button"
              title={t.moveDown}
              disabled={i === files.length - 1}
              onclick={() => move(i, 1)}>▼</button
            >
            <button type="button" title={t.remove} onclick={() => remove(i)}>×</button>
          </li>
        {/each}
      </ol>
    {/if}
    <div class="actions">
      <button type="button" onclick={add}>{t.addFiles}</button>
      <span class="spacer"></span>
      <button type="button" onclick={onclose}>{t.cancel}</button>
      <button type="button" class="primary" disabled={files.length < 2 || busy} onclick={merge}
        >{t.merge}</button
      >
    </div>
  </div>
</div>

<style>
  .files {
    margin: 0.5rem 0;
    padding-left: 1.5rem;
    max-height: 40vh;
    overflow: auto;
  }
  .files li {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    padding: 0.15rem 0;
  }
  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .muted {
    color: var(--muted);
  }
  .spacer {
    flex: 1;
  }
</style>
