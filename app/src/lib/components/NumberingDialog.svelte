<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import type { Corner } from '$lib/api';
  import { fillTemplate, scopePages, type PageScope } from '$lib/logic/content';
  import { addPageNumbers, displayName, type OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';
  import PageScopeSelect from './PageScopeSelect.svelte';

  let { doc, onclose }: { doc: OpenDoc; onclose: () => void } = $props();

  const CORNERS: Corner[] = [
    'top_left',
    'top_center',
    'top_right',
    'bottom_left',
    'bottom_center',
    'bottom_right',
  ];
  let template = $state('{n} / {total}');
  let corner = $state<Corner>('bottom_center');
  let margin = $state(28);
  let size = $state(10);
  let start = $state(1);
  let scope = $state<PageScope>('all');
  let busy = $state(false);
  let first: HTMLInputElement | undefined = $state();

  $effect(() => {
    first?.focus();
  });

  const preview = $derived(
    fillTemplate(template, start + doc.page, doc.summary.page_count, doc.summary.title ?? ''),
  );
  const canApply = $derived(preview.trim() !== '' && size >= 4 && margin >= 0 && start >= 0);

  async function apply() {
    if (!canApply) return;
    busy = true;
    const ok = await addPageNumbers(doc, {
      template,
      corner,
      margin,
      size,
      start,
      pages: scopePages(scope, doc.page, doc.selectedPages),
    });
    busy = false;
    if (ok) onclose();
  }
</script>

<div
  class="dialog-backdrop"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && onclose()}
>
  <form
    class="dialog"
    aria-labelledby="numbering-title"
    onsubmit={(e) => {
      e.preventDefault();
      apply();
    }}
  >
    <h2 id="numbering-title">{t.numberingTitle}</h2>
    <label class="row">
      {t.numberingTemplate}
      <input type="text" bind:this={first} bind:value={template} maxlength="200" />
    </label>
    <p class="muted hint">{t.numberingTemplateHint}</p>
    <p class="preview" aria-live="polite">{t.numberingPreview(preview)} — {displayName(doc)}</p>
    <label class="row">
      {t.numberingCorner}
      <select bind:value={corner}>
        {#each CORNERS as c (c)}
          <option value={c}>{t.corners[c]}</option>
        {/each}
      </select>
    </label>
    <label class="row">
      {t.numberingMargin}
      <input type="number" min="0" max="300" bind:value={margin} />
    </label>
    <label class="row">
      {t.fontSize}
      <input type="number" min="4" max="200" bind:value={size} />
    </label>
    <label class="row">
      {t.numberingStart}
      <input type="number" min="0" max="65535" bind:value={start} />
    </label>
    <PageScopeSelect {doc} bind:value={scope} />
    <div class="actions">
      <button type="button" onclick={onclose}>{t.cancel}</button>
      <button type="submit" class="primary" disabled={!canApply || busy}>{t.apply}</button>
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
  .row input[type='number'] {
    width: 5rem;
  }
  .row input[type='text'] {
    width: 14rem;
  }
  .muted {
    color: var(--muted);
  }
  .hint {
    margin: 0 0 0.4rem;
    font-size: 0.85rem;
  }
  .preview {
    margin: 0.2rem 0 0.6rem;
  }
</style>
