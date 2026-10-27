<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import type { FormField } from '$lib/api';
  import { missingRequired } from '$lib/logic/forms';
  import {
    ensureAllFormFields,
    flattenForm,
    goToPage,
    type OpenDoc,
  } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';

  let { doc }: { doc: OpenDoc } = $props();

  let fields = $state<FormField[]>([]);
  let confirming = $state(false);

  $effect(() => {
    const count = doc.summary.page_count;
    void doc.formFields; // relance après une édition (cache remplacé)
    let cancelled = false;
    (async () => {
      const all = await ensureAllFormFields(doc);
      if (cancelled) return;
      fields = all.slice(0, count).flat();
    })();
    return () => {
      cancelled = true;
    };
  });

  const missing = $derived(missingRequired(fields));
  const xfa = $derived(doc.formInfo?.form_type === 'xfa');

  function shown(f: FormField): string {
    if (f.checked !== null) return f.checked ? t.formChecked : t.formUnchecked;
    if (f.password) return '••••';
    return f.value && f.value.trim() !== '' ? f.value : t.formEmpty;
  }
  let cancelButton: HTMLButtonElement | undefined = $state();
  // La confirmation remplace le bouton qui avait le focus : on le place sur « Annuler ».
  $effect(() => {
    if (confirming) cancelButton?.focus();
  });

  /** Va à la page du champ, puis y place le focus dès que le calque l'affiche. */
  function focusField(f: FormField) {
    goToPage(doc, f.page);
    let tries = 0;
    const attempt = () => {
      const el = document.querySelector<HTMLElement>(
        `.page[data-page="${f.page}"] .form-layer [data-field="${f.index}"]`,
      );
      if (el) el.focus();
      else if (++tries < 30) requestAnimationFrame(attempt);
    };
    requestAnimationFrame(attempt);
  }

  async function flatten() {
    confirming = false;
    await flattenForm(doc);
  }
</script>

<div class="panel" role="region" aria-label={t.formTab}>
  {#if xfa}
    <p class="muted">{t.formXfa}</p>
  {/if}
  <p class="summary">
    {t.formFieldCount(fields.length)}
    {#if missing.length > 0}
      · <span class="missing">{t.formMissingRequired(missing.length)}</span>
    {/if}
  </p>
  <ul role="list">
    {#each fields as f (`${f.page}-${f.index}`)}
      <li class:missing={missing.includes(f)}>
        <button type="button" class="item" onclick={() => focusField(f)}>
          <span class="name">{f.name}{f.required ? ' *' : ''}</span>
          <span class="value">{shown(f)}</span>
          <span class="meta"
            >{t.annotationOnPage(f.page + 1)} · {t.fieldKind[f.kind] ?? f.kind}</span
          >
        </button>
      </li>
    {/each}
  </ul>
  <div class="actions">
    {#if confirming}
      <span>{t.flattenConfirm}</span>
      <button type="button" bind:this={cancelButton} onclick={() => (confirming = false)}
        >{t.cancel}</button
      >
      <button type="button" class="primary" onclick={flatten}>{t.flatten}</button>
    {:else}
      <button
        type="button"
        disabled={fields.length === 0 || xfa}
        onclick={() => (confirming = true)}>{t.flatten}…</button
      >
    {/if}
  </div>
</div>

<style>
  .panel {
    padding: 0.25rem 0 0.75rem;
  }
  .muted,
  .summary {
    color: var(--muted);
    padding: 0.5rem 0.75rem;
    margin: 0;
    font-size: 0.8rem;
  }
  .missing {
    color: #e53935;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li.missing {
    border-left: 3px solid #e53935;
  }
  .item {
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    text-align: left;
    padding: 0.35rem 0.6rem;
    border-radius: 0;
    background: none;
  }
  .name {
    font-weight: 600;
  }
  .value {
    font-size: 0.85rem;
    word-break: break-word;
  }
  .meta {
    font-size: 0.75rem;
    color: var(--muted);
  }
  .actions {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    flex-wrap: wrap;
    padding: 0.6rem 0.75rem;
    font-size: 0.85rem;
  }
</style>
