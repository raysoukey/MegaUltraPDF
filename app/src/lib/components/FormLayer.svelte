<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import type { FormField } from '$lib/api';
  import { fieldInputType } from '$lib/logic/forms';
  import { pdfRectToCss } from '$lib/logic/geometry';
  import { ensureFormFields, setFieldValue, type OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';

  let { doc, index }: { doc: OpenDoc; index: number } = $props();

  const page = $derived(doc.summary.pages[index]);
  let fields = $state<FormField[]>([]);

  // Rechargé après chaque édition (le cache `doc.formFields` est vidé par l'état).
  $effect(() => {
    let cancelled = false;
    ensureFormFields(doc, index).then((list) => {
      if (!cancelled) fields = list;
    });
    return () => {
      cancelled = true;
    };
  });

  const xfa = $derived(doc.formInfo?.form_type === 'xfa');

  function css(f: FormField) {
    return pdfRectToCss(f.rect, page, doc.scale, doc.rotation);
  }
  function fontSize(f: FormField): number {
    const c = css(f);
    return Math.max(9, Math.min(c.height * 0.7, 24 * doc.scale));
  }
  function commitText(f: FormField, e: Event) {
    const next = (e.currentTarget as HTMLInputElement | HTMLTextAreaElement).value;
    if (next === (f.value ?? '')) return;
    void setFieldValue(doc, index, f.index, { kind: 'text', text: next });
  }
  function commitChecked(f: FormField, e: Event) {
    const next = (e.currentTarget as HTMLInputElement).checked;
    void setFieldValue(doc, index, f.index, { kind: 'checked', checked: next });
  }
  function commitSelect(f: FormField, e: Event) {
    const next = (e.currentTarget as HTMLSelectElement).value;
    if (next === (f.value ?? '')) return;
    void setFieldValue(doc, index, f.index, { kind: 'text', text: next });
  }
  function commitRadio(f: FormField) {
    if (f.checked) return;
    void setFieldValue(doc, index, f.index, { kind: 'checked', checked: true });
  }
  function title(f: FormField): string {
    if (xfa) return t.formXfa;
    if (f.read_only) return t.formReadOnly;
    return f.required ? `${f.name} (${t.formRequired})` : f.name;
  }
</script>

<div class="form-layer" aria-label={t.formFields}>
  {#each fields as f (f.index)}
    {@const kind = fieldInputType(f)}
    {@const c = css(f)}
    {#if kind !== 'none'}
      <div
        class="field"
        class:required={f.required}
        style:left="{c.left}px"
        style:top="{c.top}px"
        style:width="{c.width}px"
        style:height="{c.height}px"
      >
        {#if kind === 'text' || kind === 'password'}
          <input
            type={kind}
            value={f.value ?? ''}
            readonly={f.read_only || xfa}
            aria-label={f.name}
            title={title(f)}
            style:font-size="{fontSize(f)}px"
            onchange={(e) => commitText(f, e)}
          />
        {:else if kind === 'textarea'}
          <textarea
            value={f.value ?? ''}
            readonly={f.read_only || xfa}
            aria-label={f.name}
            title={title(f)}
            style:font-size="{fontSize(f)}px"
            onchange={(e) => commitText(f, e)}
          ></textarea>
        {:else if kind === 'checkbox'}
          <input
            type="checkbox"
            checked={f.checked ?? false}
            disabled={f.read_only || xfa}
            aria-label={f.name}
            title={title(f)}
            onchange={(e) => commitChecked(f, e)}
          />
        {:else if kind === 'radio'}
          <input
            type="radio"
            name="radio-{index}-{f.name}"
            checked={f.checked ?? false}
            disabled={f.read_only || xfa}
            aria-label={f.name}
            title={title(f)}
            onchange={() => commitRadio(f)}
          />
        {:else if kind === 'select'}
          <select
            disabled={f.read_only || xfa}
            aria-label={f.name}
            title={title(f)}
            style:font-size="{fontSize(f)}px"
            onchange={(e) => commitSelect(f, e)}
          >
            {#each f.options as o, i (i)}
              <option selected={o.selected || o.label === f.value}>{o.label}</option>
            {/each}
          </select>
        {/if}
      </div>
    {/if}
  {/each}
</div>

<style>
  .form-layer {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  .field {
    position: absolute;
    display: flex;
    align-items: stretch;
    pointer-events: auto;
    box-sizing: border-box;
  }
  .field input,
  .field textarea,
  .field select {
    width: 100%;
    height: 100%;
    box-sizing: border-box;
    font: inherit;
    padding: 0 2px;
    background: rgba(230, 240, 255, 0.85);
    border: 1px solid rgba(37, 99, 235, 0.45);
    border-radius: 2px;
    color: #111;
  }
  .field textarea {
    resize: none;
  }
  .field input[type='checkbox'],
  .field input[type='radio'] {
    width: 100%;
    height: 100%;
    margin: 0;
    accent-color: var(--accent);
  }
  .field.required input,
  .field.required textarea {
    border-color: #e53935;
  }
  .field input:read-only,
  .field select:disabled,
  .field input:disabled {
    background: rgba(230, 230, 230, 0.75);
    color: #444;
  }
</style>
