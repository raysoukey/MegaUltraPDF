<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { trapFocus } from '$lib/actions/trapFocus';
  import { pickDirectory, pickSaveFile } from '$lib/api';
  import { scopePages, type PageScope } from '$lib/logic/content';
  import { defaultPrefix, exportProblems } from '$lib/logic/export';
  import { fileName } from '$lib/logic/labels';
  import { exportImagesTo, exportTextTo, type OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';
  import PageScopeSelect from './PageScopeSelect.svelte';

  let { doc, onclose }: { doc: OpenDoc; onclose: () => void } = $props();

  let mode = $state<'images' | 'text'>('images');
  let scope = $state<PageScope>('all');
  let format = $state<'png' | 'jpeg'>('png');
  let dpi = $state(150);
  let quality = $state(85);
  let withAnnotations = $state(true);
  let dir = $state<string | null>(null);
  // svelte-ignore state_referenced_locally
  let prefix = $state(defaultPrefix(doc.summary.path, doc.summary.title));
  let force = $state(false);
  let separators = $state(true);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let firstTab: HTMLButtonElement | undefined = $state();

  $effect(() => {
    firstTab?.focus();
  });

  const problems = $derived(exportProblems({ dpi, quality, prefix }));
  const canExport = $derived(
    mode === 'text' ? true : problems.length === 0 && dir !== null && !busy,
  );

  async function chooseDir() {
    const picked = await pickDirectory();
    if (picked) dir = picked;
  }

  async function run() {
    busy = true;
    error = null;
    const pages = scopePages(scope, doc.page, doc.selectedPages);
    try {
      if (mode === 'images') {
        if (!dir) return;
        const written = await exportImagesTo(
          doc,
          {
            pages,
            format: format === 'png' ? { kind: 'png' } : { kind: 'jpeg', quality },
            dpi,
            with_annotations: withAnnotations,
          },
          dir,
          prefix.trim(),
          force,
        );
        if (written !== null) onclose();
      } else {
        const path = await pickSaveFile(`${prefix.trim()}.txt`);
        if (!path) return;
        const ok = await exportTextTo(doc, pages, separators, path);
        if (ok) onclose();
      }
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
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="export-title" use:trapFocus>
    <form
      class="dialog-form"
      onsubmit={(e) => {
        e.preventDefault();
        run();
      }}
    >
      <h2 id="export-title">{t.exportTitle}</h2>
      <div class="tabs" role="tablist">
        <button
          type="button"
          role="tab"
          bind:this={firstTab}
          aria-selected={mode === 'images'}
          class:active={mode === 'images'}
          onclick={() => (mode = 'images')}>{t.exportImages}</button
        >
        <button
          type="button"
          role="tab"
          aria-selected={mode === 'text'}
          class:active={mode === 'text'}
          onclick={() => (mode = 'text')}>{t.exportText}</button
        >
      </div>
      <PageScopeSelect {doc} bind:value={scope} />
      {#if mode === 'images'}
        <label class="row">
          {t.exportFormat}
          <select bind:value={format}>
            <option value="png">PNG</option>
            <option value="jpeg">JPEG</option>
          </select>
        </label>
        {#if format === 'jpeg'}
          <label class="row">
            {t.exportQuality}
            <input type="number" min="10" max="100" bind:value={quality} />
          </label>
        {/if}
        <label class="row">
          {t.exportDpi}
          <select bind:value={dpi}>
            {#each [72, 96, 150, 200, 300, 600] as d (d)}
              <option value={d}>{d}</option>
            {/each}
          </select>
        </label>
        <label class="row">
          {t.exportPrefix}
          <input type="text" bind:value={prefix} />
        </label>
        <label class="row check">
          <input type="checkbox" bind:checked={withAnnotations} />
          {t.exportWithAnnotations}
        </label>
        <label class="row check">
          <input type="checkbox" bind:checked={force} />
          {t.exportOverwrite}
        </label>
        <div class="row">
          <button type="button" onclick={chooseDir}>{t.chooseFolder}</button>
          <span class="dir" title={dir ?? ''}>{dir ? fileName(dir) : t.noFolderChosen}</span>
        </div>
      {:else}
        <label class="row check">
          <input type="checkbox" bind:checked={separators} />
          {t.exportSeparators}
        </label>
        <p class="muted">{t.exportTextHint}</p>
      {/if}
      {#if error}
        <p class="error" role="alert">{error}</p>
      {/if}
      <div class="actions">
        <button type="button" onclick={onclose}>{t.cancel}</button>
        <button type="submit" class="primary" disabled={!canExport || busy}>{t.exportAction}</button
        >
      </div>
    </form>
  </div>
</div>

<style>
  .tabs {
    display: flex;
    gap: 0.25rem;
    margin-bottom: 0.5rem;
  }
  .tabs button.active {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
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
    width: 5rem;
  }
  .row input[type='text'] {
    width: 14rem;
  }
  .dir {
    color: var(--muted);
    max-width: 16rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .muted {
    color: var(--muted);
    font-size: 0.85rem;
  }
  .error {
    color: var(--danger-text);
  }
</style>
