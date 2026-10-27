<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { trapFocus } from '$lib/actions/trapFocus';
  import { scopePages, type PageScope } from '$lib/logic/content';
  import { addWatermark, type OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';
  import PageScopeSelect from './PageScopeSelect.svelte';

  let { doc, onclose }: { doc: OpenDoc; onclose: () => void } = $props();

  let text = $state(t.watermarkDefaultText);
  let size = $state(60);
  let opacity = $state(0.3);
  let angle = $state(45);
  let color = $state('#808080');
  let scope = $state<PageScope>('all');
  let busy = $state(false);
  let first: HTMLInputElement | undefined = $state();

  $effect(() => {
    first?.focus();
  });

  function hexToColor(hex: string) {
    const n = Number.parseInt(hex.slice(1), 16);
    return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 };
  }

  const canApply = $derived(text.trim() !== '' && opacity > 0 && opacity <= 1 && size >= 4);

  async function apply() {
    if (!canApply) return;
    busy = true;
    const ok = await addWatermark(doc, {
      text: text.trim(),
      size,
      opacity,
      angle_deg: angle,
      color: hexToColor(color),
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
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="watermark-title"
    use:trapFocus
  >
    <form
      class="dialog-form"
      onsubmit={(e) => {
        e.preventDefault();
        apply();
      }}
    >
      <h2 id="watermark-title">{t.watermarkTitle}</h2>
      <label class="row">
        {t.watermarkText}
        <input type="text" bind:this={first} bind:value={text} maxlength="200" />
      </label>
      <label class="row">
        {t.watermarkSize}
        <input type="number" min="4" max="400" bind:value={size} />
      </label>
      <label class="row">
        {t.opacity}
        <input type="range" min="0.05" max="1" step="0.05" bind:value={opacity} />
        <span class="muted">{Math.round(opacity * 100)} %</span>
      </label>
      <label class="row">
        {t.watermarkAngle}
        <input type="number" min="-90" max="90" bind:value={angle} />
      </label>
      <label class="row">
        {t.color}
        <input type="color" bind:value={color} />
      </label>
      <PageScopeSelect {doc} bind:value={scope} />
      <div class="actions">
        <button type="button" onclick={onclose}>{t.cancel}</button>
        <button type="submit" class="primary" disabled={!canApply || busy}>{t.apply}</button>
      </div>
    </form>
  </div>
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
  .muted {
    color: var(--muted);
    min-width: 3rem;
    text-align: right;
  }
</style>
