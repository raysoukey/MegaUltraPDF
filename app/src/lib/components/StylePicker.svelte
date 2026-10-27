<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import type { Color } from '$lib/api';
  import { currentStyle, familyOf, toolState, updateStyle } from '$lib/state/tools.svelte';
  import { t } from '$lib/strings';

  const SWATCHES: Color[] = [
    { r: 255, g: 212, b: 0 },
    { r: 229, g: 57, b: 53 },
    { r: 30, g: 136, b: 229 },
    { r: 67, g: 160, b: 71 },
    { r: 251, g: 140, b: 0 },
    { r: 142, g: 36, b: 170 },
    { r: 0, g: 0, b: 0 },
    { r: 255, g: 255, b: 255 },
  ];

  const style = $derived(currentStyle());
  const family = $derived(familyOf(toolState.tool));

  function hex(c: Color): string {
    return '#' + [c.r, c.g, c.b].map((v) => v.toString(16).padStart(2, '0')).join('');
  }
  function fromHex(h: string): Color {
    return {
      r: Number.parseInt(h.slice(1, 3), 16),
      g: Number.parseInt(h.slice(3, 5), 16),
      b: Number.parseInt(h.slice(5, 7), 16),
    };
  }
  function same(a: Color, b: Color | null): boolean {
    return !!b && a.r === b.r && a.g === b.g && a.b === b.b;
  }
</script>

<div class="picker">
  <div class="swatches" role="radiogroup" aria-label={t.color}>
    {#each SWATCHES as c (hex(c))}
      <button
        type="button"
        role="radio"
        class="swatch"
        class:active={same(c, style.color)}
        aria-checked={same(c, style.color)}
        aria-label={hex(c)}
        style:background={hex(c)}
        onclick={() => updateStyle({ color: c })}
      ></button>
    {/each}
    <input
      type="color"
      aria-label={t.customColor}
      title={t.customColor}
      value={hex(style.color)}
      onchange={(e) => updateStyle({ color: fromHex((e.currentTarget as HTMLInputElement).value) })}
    />
  </div>
  <label title={t.opacity}>
    {t.opacity}
    <input
      type="range"
      min="0.1"
      max="1"
      step="0.1"
      value={style.opacity}
      oninput={(e) => updateStyle({ opacity: Number((e.currentTarget as HTMLInputElement).value) })}
    />
  </label>
  {#if family === 'shape'}
    <label title={t.strokeWidth}>
      {t.strokeWidth}
      <input
        type="range"
        min="1"
        max="12"
        step="1"
        value={style.stroke_width}
        oninput={(e) =>
          updateStyle({ stroke_width: Number((e.currentTarget as HTMLInputElement).value) })}
      />
    </label>
    {#if toolState.tool === 'rect' || toolState.tool === 'ellipse'}
      <label class="fill">
        <input
          type="checkbox"
          checked={style.fill !== null}
          onchange={(e) =>
            updateStyle({
              fill: (e.currentTarget as HTMLInputElement).checked ? style.color : null,
            })}
        />
        {t.fillColor}
      </label>
    {/if}
  {/if}
  {#if family === 'text'}
    <label title={t.fontSize}>
      {t.fontSize}
      <input
        type="number"
        min="6"
        max="72"
        value={style.font_size}
        onchange={(e) =>
          updateStyle({
            font_size: Math.max(6, Number((e.currentTarget as HTMLInputElement).value) || 12),
          })}
      />
    </label>
  {/if}
</div>

<style>
  .picker {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex-wrap: wrap;
  }
  .swatches {
    display: flex;
    align-items: center;
    gap: 3px;
  }
  .swatch {
    width: 18px;
    height: 18px;
    padding: 0;
    border-radius: 50%;
    border: 2px solid var(--panel-border);
  }
  .swatch.active {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--accent-soft);
  }
  input[type='color'] {
    width: 24px;
    height: 22px;
    padding: 0;
    border: none;
    background: none;
  }
  label {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.8rem;
    color: var(--muted);
  }
  input[type='range'] {
    width: 70px;
  }
  input[type='number'] {
    width: 3.5rem;
  }
</style>
