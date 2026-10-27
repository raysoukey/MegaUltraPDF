<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { trapFocus } from '$lib/actions/trapFocus';
  import { comparePages, isAppError, type ComparedPages } from '$lib/api';
  import { displayName, docsState, openPath, type OpenDoc } from '$lib/state/documents.svelte';
  import { pickPdfFiles } from '$lib/api';
  import { t } from '$lib/strings';

  let { doc, onclose }: { doc: OpenDoc; onclose: () => void } = $props();

  const others = $derived(docsState.docs.filter((d) => d.summary.id !== doc.summary.id));
  let otherId = $state<number | null>(null);
  let page = $state(0);
  let result = $state<ComparedPages | null>(null);
  let url = $state<string | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  $effect(() => {
    if (otherId === null && others.length > 0) otherId = others[0].summary.id;
  });
  const other = $derived(others.find((d) => d.summary.id === otherId) ?? null);
  const maxPage = $derived(
    other ? Math.min(doc.summary.page_count, other.summary.page_count) - 1 : 0,
  );

  async function openOther() {
    const paths = await pickPdfFiles();
    if (paths.length === 0) return;
    const ok = await openPath(paths[0]);
    if (ok) {
      const opened = docsState.docs.find((d) => d.summary.path === paths[0]);
      if (opened) otherId = opened.summary.id;
      // Comparer garde le document courant actif.
      docsState.activeId = doc.summary.id;
    }
  }

  async function compare() {
    if (!other) return;
    busy = true;
    error = null;
    try {
      const r = await comparePages(doc.summary.id, page, other.summary.id, page, 1.5);
      if (url) URL.revokeObjectURL(url);
      result = r;
      url = URL.createObjectURL(new Blob([r.png], { type: 'image/png' }));
    } catch (e) {
      error = isAppError(e) ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  function step(dir: 1 | -1) {
    page = Math.max(0, Math.min(maxPage, page + dir));
    void compare();
  }

  $effect(() => {
    return () => {
      if (url) URL.revokeObjectURL(url);
    };
  });
</script>

<div
  class="dialog-backdrop"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && onclose()}
>
  <div
    class="dialog wide"
    role="dialog"
    aria-modal="true"
    use:trapFocus
    aria-labelledby="compare-title"
  >
    <h2 id="compare-title">{t.compareTitle}</h2>
    <div class="row">
      <span>{t.compareWith}</span>
      {#if others.length > 0}
        <select bind:value={otherId}>
          {#each others as d (d.summary.id)}
            <option value={d.summary.id}>{displayName(d)}</option>
          {/each}
        </select>
      {:else}
        <span class="muted">{t.compareNoOther}</span>
      {/if}
      <button type="button" onclick={openOther}>{t.compareOpenFile}</button>
    </div>
    <div class="row">
      <button
        type="button"
        aria-label={t.previousPage}
        title={t.previousPage}
        disabled={page === 0}
        onclick={() => step(-1)}>‹</button
      >
      <span>{t.pageOf(page + 1, maxPage + 1)}</span>
      <button
        type="button"
        aria-label={t.nextPage}
        title={t.nextPage}
        disabled={page >= maxPage}
        onclick={() => step(1)}>›</button
      >
      <button type="button" class="primary" disabled={!other || busy} onclick={compare}
        >{t.compareAction}</button
      >
      {#if result}
        <span role="status" class:differs={result.differing_pixels > 0}>
          {t.compareResult(result.differing_pixels, result.total_pixels)}
        </span>
      {/if}
    </div>
    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
    {#if url}
      <div class="diff">
        <img src={url} alt={t.compareImageAlt} />
      </div>
    {:else}
      <p class="muted">{t.compareHint}</p>
    {/if}
    <div class="actions">
      <button type="button" onclick={onclose}>{t.close}</button>
    </div>
  </div>
</div>

<style>
  .wide {
    min-width: 560px;
    max-width: 90vw;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin: 0.4rem 0;
    flex-wrap: wrap;
  }
  .muted {
    color: var(--muted);
    font-size: 0.85rem;
  }
  .differs {
    color: var(--danger-text);
  }
  .diff {
    max-height: 60vh;
    overflow: auto;
    border: 1px solid var(--panel-border);
    background: #fff;
  }
  .diff img {
    display: block;
    max-width: 100%;
    height: auto;
  }
  .error {
    color: var(--danger-text);
  }
</style>
