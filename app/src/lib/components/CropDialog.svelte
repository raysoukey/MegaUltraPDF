<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { cropMarginsValid, scopePages, type PageScope } from '$lib/logic/content';
  import { cropPages, type OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';
  import PageScopeSelect from './PageScopeSelect.svelte';

  let { doc, onclose }: { doc: OpenDoc; onclose: () => void } = $props();

  let left = $state(0);
  let bottom = $state(0);
  let right = $state(0);
  let top = $state(0);
  let scope = $state<PageScope>('current');
  let busy = $state(false);
  let first: HTMLInputElement | undefined = $state();

  $effect(() => {
    first?.focus();
  });

  const margins = $derived<[number, number, number, number]>([left, bottom, right, top]);
  // Validé sur la première page visée (les autres pages d'un même document font en
  // général la même taille) ; le backend revérifie chacune.
  const firstPage = $derived((scopePages(scope, doc.page, doc.selectedPages) ?? [0])[0] ?? 0);
  const valid = $derived(cropMarginsValid(margins, doc.summary.pages[firstPage]));

  function reset() {
    left = bottom = right = top = 0;
  }

  async function apply() {
    if (!valid) return;
    busy = true;
    const ok = await cropPages(doc, {
      pages: scopePages(scope, doc.page, doc.selectedPages),
      margins,
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
    aria-labelledby="crop-title"
    onsubmit={(e) => {
      e.preventDefault();
      apply();
    }}
  >
    <h2 id="crop-title">{t.cropTitle}</h2>
    <fieldset>
      <legend>{t.cropMargins}</legend>
      <div class="grid">
        <label
          >{t.cropLeft}<input type="number" min="0" bind:this={first} bind:value={left} /></label
        >
        <label>{t.cropTop}<input type="number" min="0" bind:value={top} /></label>
        <label>{t.cropRight}<input type="number" min="0" bind:value={right} /></label>
        <label>{t.cropBottom}<input type="number" min="0" bind:value={bottom} /></label>
      </div>
      <button type="button" onclick={reset}>{t.cropReset}</button>
    </fieldset>
    {#if !valid}
      <p class="error" role="alert">{t.cropInvalid}</p>
    {/if}
    <PageScopeSelect {doc} bind:value={scope} />
    <div class="actions">
      <button type="button" onclick={onclose}>{t.cancel}</button>
      <button type="submit" class="primary" disabled={!valid || busy}>{t.crop}</button>
    </div>
  </form>
</div>

<style>
  fieldset {
    border: 1px solid var(--panel-border);
    border-radius: 6px;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.4rem 1rem;
  }
  .grid label {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }
  .grid input {
    width: 5rem;
  }
  .error {
    color: var(--danger-text);
    margin: 0.4rem 0 0;
  }
</style>
