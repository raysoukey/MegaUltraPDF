<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { trapFocus } from '$lib/actions/trapFocus';
  import { isAppError, pickDirectory, splitDocument, splitPlan, type SplitMode } from '$lib/api';
  import { fileName } from '$lib/logic/labels';
  import { showNotice, type OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';

  let { doc, onclose }: { doc: OpenDoc; onclose: () => void } = $props();

  let kind = $state<SplitMode['kind']>('every');
  let spec = $state('');
  let every = $state(1);
  let outDir = $state<string | null>(null);
  // svelte-ignore state_referenced_locally
  let prefix = $state(
    doc.summary.path ? fileName(doc.summary.path).replace(/\.pdf$/i, '') : t.splitDefaultPrefix,
  );
  let plan = $state<number[][] | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  function mode(): SplitMode {
    if (kind === 'ranges') return { kind: 'ranges', spec };
    if (kind === 'every') return { kind: 'every', n: Math.max(1, Math.floor(every)) };
    return { kind: 'bookmarks' };
  }

  // Prévisualisation : recalculée à chaque changement de mode ou de valeur.
  $effect(() => {
    const m = mode();
    // Lu ici pour que l'aperçu soit recalculé si le document change sous le dialogue.
    void doc.summary.page_count;
    let cancelled = false;
    splitPlan(doc.summary.id, m)
      .then((p) => {
        if (cancelled) return;
        plan = p;
        error = null;
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        plan = null;
        error = isAppError(e) ? e.message : String(e);
      });
    return () => {
      cancelled = true;
    };
  });

  async function chooseDir() {
    const picked = await pickDirectory();
    if (picked) outDir = picked;
  }

  async function split() {
    if (!plan || !outDir) return;
    busy = true;
    try {
      const files = await splitDocument(
        doc.summary.id,
        plan,
        outDir,
        prefix.trim() || t.splitDefaultPrefix,
      );
      showNotice(t.splitDone(files.length));
      onclose();
    } catch (e) {
      error = isAppError(e) ? e.message : String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div
  class="dialog-backdrop"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && onclose()}
>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="split-title" use:trapFocus>
    <form
      class="dialog-form"
      onsubmit={(e) => {
        e.preventDefault();
        split();
      }}
    >
      <h2 id="split-title">{t.splitTitle}</h2>
      <fieldset>
        <legend>{t.splitMode}</legend>
        <label>
          <input type="radio" name="split-mode" value="every" bind:group={kind} />
          {t.splitEvery}
          <input
            type="number"
            min="1"
            max={doc.summary.page_count}
            bind:value={every}
            disabled={kind !== 'every'}
            aria-label={t.splitEveryLabel}
          />
        </label>
        <label>
          <input type="radio" name="split-mode" value="ranges" bind:group={kind} />
          {t.splitRanges}
          <input
            type="text"
            bind:value={spec}
            placeholder={t.splitRangesPlaceholder}
            disabled={kind !== 'ranges'}
            aria-label={t.splitRanges}
          />
        </label>
        <label>
          <input type="radio" name="split-mode" value="bookmarks" bind:group={kind} />
          {t.splitBookmarks}
        </label>
      </fieldset>
      <p class="preview" aria-live="polite">
        {#if error}
          <span class="error">{error}</span>
        {:else if plan}
          {t.splitPreview(plan.length)}
        {/if}
      </p>
      <label class="row">
        {t.splitPrefix}
        <input type="text" bind:value={prefix} />
      </label>
      <div class="row">
        <button type="button" onclick={chooseDir}>{t.chooseFolder}</button>
        <span class="dir" title={outDir ?? ''}>{outDir ?? t.noFolderChosen}</span>
      </div>
      <div class="actions">
        <button type="button" onclick={onclose}>{t.cancel}</button>
        <button type="submit" class="primary" disabled={!plan || !outDir || busy}>{t.split}</button>
      </div>
    </form>
  </div>
</div>

<style>
  fieldset {
    border: 1px solid var(--panel-border);
    border-radius: 6px;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  fieldset label {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
  input[type='number'] {
    width: 4rem;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin: 0.5rem 0;
  }
  .row input[type='text'] {
    flex: 1;
  }
  .dir {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
  }
  .preview {
    min-height: 1.2em;
    margin: 0.5rem 0;
  }
  .error {
    color: var(--danger-text);
  }
</style>
