<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { trapFocus } from '$lib/actions/trapFocus';
  import type { FontFamily, TextStyle } from '$lib/api';
  import { cancelTextInsert, insertText, type OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';

  let { doc, page, at }: { doc: OpenDoc; page: number; at: [number, number] } = $props();

  let text = $state('');
  let family = $state<FontFamily>('helvetica');
  let bold = $state(false);
  let italic = $state(false);
  let size = $state(14);
  let color = $state('#111111');
  let busy = $state(false);
  let area: HTMLTextAreaElement | undefined = $state();

  $effect(() => {
    area?.focus();
  });

  function hexToColor(hex: string) {
    const n = Number.parseInt(hex.slice(1), 16);
    return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 };
  }

  const canApply = $derived(text.trim() !== '' && size >= 4 && size <= 400);

  async function apply() {
    if (!canApply) return;
    busy = true;
    const style: TextStyle = { family, bold, italic, size, color: hexToColor(color) };
    await insertText(doc, page, at, text, style);
    busy = false;
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      apply();
    }
  }
</script>

<div class="dialog-backdrop" role="presentation">
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="text-insert-title"
    use:trapFocus
  >
    <form
      class="dialog-form"
      onsubmit={(e) => {
        e.preventDefault();
        apply();
      }}
    >
      <h2 id="text-insert-title">{t.textInsertTitle}</h2>
      <label class="col">
        {t.textInsertText}
        <textarea bind:this={area} bind:value={text} rows="3" onkeydown={onKey}></textarea>
      </label>
      <label class="row">
        {t.fontFamily}
        <select bind:value={family}>
          {#each ['helvetica', 'times', 'courier', 'unicode'] as f (f)}
            <option value={f}>{t.fontFamilies[f]}</option>
          {/each}
        </select>
      </label>
      <p class="hint">{t.fontUnicodeHint}</p>
      <div class="row">
        <label><input type="checkbox" bind:checked={bold} /> {t.bold}</label>
        <label><input type="checkbox" bind:checked={italic} /> {t.italic}</label>
        <label>
          {t.fontSize}
          <input type="number" min="4" max="400" step="1" bind:value={size} />
        </label>
        <label>
          {t.color}
          <input type="color" bind:value={color} />
        </label>
      </div>
      <div class="actions">
        <button type="button" onclick={() => cancelTextInsert(doc)}>{t.cancel}</button>
        <button type="submit" class="primary" disabled={!canApply || busy}>{t.insert}</button>
      </div>
    </form>
  </div>
</div>

<style>
  .hint {
    margin: 0 0 0.4rem;
    color: var(--muted);
    font-size: 0.8rem;
    max-width: 24rem;
  }
  .col {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    margin: 0.4rem 0;
  }
  .col textarea {
    min-width: 320px;
    font: inherit;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin: 0.4rem 0;
    flex-wrap: wrap;
  }
  .row input[type='number'] {
    width: 4.5rem;
  }
</style>
