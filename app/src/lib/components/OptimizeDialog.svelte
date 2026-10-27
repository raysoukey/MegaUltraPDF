<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { isAppError, optimizePreview, type OptimizeOptions, type OptimizeReport } from '$lib/api';
  import { optimizeDoc, type OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';

  let { doc, onclose }: { doc: OpenDoc; onclose: () => void } = $props();

  let recompress = $state(true);
  let quality = $state(75);
  let downsample = $state(false);
  let maxPx = $state(2048);
  let compressStreams = $state(true);
  let busy = $state(false);
  let report = $state<OptimizeReport | null>(null);
  let error = $state<string | null>(null);
  let done = $state(false);
  let first: HTMLInputElement | undefined = $state();

  $effect(() => {
    first?.focus();
  });

  function options(): OptimizeOptions {
    return {
      image_quality: recompress ? quality : null,
      max_image_px: downsample ? maxPx : null,
      compress_streams: compressStreams,
    };
  }
  // Toute modification des réglages invalide l'analyse précédente.
  function invalidate() {
    report = null;
    error = null;
  }
  const gain = $derived(report !== null && report.bytes_after < report.bytes_before);
  const pct = $derived(
    report && report.bytes_before > 0
      ? Math.round((100 * (report.bytes_before - report.bytes_after)) / report.bytes_before)
      : 0,
  );

  async function analyze() {
    busy = true;
    error = null;
    try {
      report = await optimizePreview(doc.summary.id, options());
    } catch (e) {
      error = isAppError(e) ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function apply() {
    busy = true;
    const ok = await optimizeDoc(doc, options());
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
      if (done) onclose();
      else if (gain) apply();
      else analyze();
    }}
  >
    <h2 id="optimize-title">{t.optimizeTitle}</h2>
    {#if done}
      <p role="status">{t.optimizeDone}</p>
      <p class="muted">{t.optimizeSaveHint}</p>
    {:else}
      <p class="muted">{t.optimizeHint}</p>
      <label class="row check">
        <input type="checkbox" bind:this={first} bind:checked={recompress} onchange={invalidate} />
        {t.optimizeRecompress}
      </label>
      <label class="row">
        {t.optimizeQuality}
        <input
          type="number"
          min="10"
          max="100"
          bind:value={quality}
          disabled={!recompress}
          oninput={invalidate}
        />
      </label>
      <label class="row check">
        <input type="checkbox" bind:checked={downsample} onchange={invalidate} />
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
          oninput={invalidate}
        />
      </label>
      <label class="row check">
        <input type="checkbox" bind:checked={compressStreams} onchange={invalidate} />
        {t.optimizeCompressStreams}
      </label>
      {#if report}
        <p class="report" role="status">
          {gain
            ? t.optimizeReport(
                t.bytes(report.bytes_before),
                t.bytes(report.bytes_after),
                pct,
                report.images_recompressed,
              )
            : t.optimizeNothing}
        </p>
      {/if}
      {#if error}
        <p class="error" role="alert">{error}</p>
      {/if}
    {/if}
    <div class="actions">
      {#if !done}
        <button type="button" onclick={onclose}>{t.cancel}</button>
        <button type="button" disabled={busy} onclick={analyze}>{t.optimizeAnalyze}</button>
      {/if}
      <button type="submit" class="primary" disabled={busy || (!done && !gain)}
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
  .report {
    font-weight: 500;
  }
  .error {
    color: var(--danger-text);
  }
</style>
