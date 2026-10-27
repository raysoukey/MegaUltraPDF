<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { fileSize } from '$lib/api';
  import { optimizeDoc, type OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';

  let { doc, onclose }: { doc: OpenDoc; onclose: () => void } = $props();

  let recompress = $state(true);
  let quality = $state(75);
  let downsample = $state(false);
  let maxPx = $state(2048);
  let compressStreams = $state(true);
  let busy = $state(false);
  let before = $state<number | null>(null);
  let done = $state(false);
  let first: HTMLInputElement | undefined = $state();

  $effect(() => {
    first?.focus();
    if (doc.summary.path)
      fileSize(doc.summary.path)
        .then((s) => (before = s))
        .catch(() => (before = null));
  });

  async function run() {
    busy = true;
    const ok = await optimizeDoc(doc, {
      image_quality: recompress ? quality : null,
      max_image_px: downsample ? maxPx : null,
      compress_streams: compressStreams,
    });
    busy = false;
    if (ok) done = true;
  }
</script>

<div
  class="dialog-backdrop"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && onclose()}
>
  <form
    class="dialog"
    aria-labelledby="optimize-title"
    onsubmit={(e) => {
      e.preventDefault();
      if (!done) run();
      else onclose();
    }}
  >
    <h2 id="optimize-title">{t.optimizeTitle}</h2>
    {#if done}
      <p role="status">{t.optimizeDone}</p>
      <p class="muted">{t.optimizeSaveHint}</p>
    {:else}
      <p class="muted">
        {t.optimizeHint}{#if before !== null}
          · {t.optimizeCurrentSize(t.bytes(before))}{/if}
      </p>
      <label class="row check">
        <input type="checkbox" bind:this={first} bind:checked={recompress} />
        {t.optimizeRecompress}
      </label>
      <label class="row">
        {t.optimizeQuality}
        <input type="number" min="10" max="100" bind:value={quality} disabled={!recompress} />
      </label>
      <label class="row check">
        <input type="checkbox" bind:checked={downsample} />
        {t.optimizeDownsample}
      </label>
      <label class="row">
        {t.optimizeMaxPx}
        <input
          type="number"
          min="256"
          max="8192"
          step="256"
          bind:value={maxPx}
          disabled={!downsample}
        />
      </label>
      <label class="row check">
        <input type="checkbox" bind:checked={compressStreams} />
        {t.optimizeCompressStreams}
      </label>
    {/if}
    <div class="actions">
      {#if !done}
        <button type="button" onclick={onclose}>{t.cancel}</button>
      {/if}
      <button type="submit" class="primary" disabled={busy}
        >{done ? t.close : t.optimizeAction}</button
      >
    </div>
  </form>
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    margin: 0.4rem 0;
  }
  .row.check {
    justify-content: flex-start;
  }
  .row input[type='number'] {
    width: 6rem;
  }
  .muted {
    color: var(--muted);
    font-size: 0.85rem;
    max-width: 28rem;
  }
</style>
