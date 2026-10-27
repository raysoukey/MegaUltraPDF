<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { pickImageFile, type Rect } from '$lib/api';
  import { canvasStrokesToPdf } from '$lib/logic/forms';
  import { fileName } from '$lib/logic/labels';
  import { cancelSignature, insertSignature, type OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';

  let { doc, page, rect }: { doc: OpenDoc; page: number; rect: Rect } = $props();

  const W = 400;
  const H = 150;
  let mode = $state<'draw' | 'image'>('draw');
  let imagePath = $state<string | null>(null);
  let strokes = $state<{ x: number; y: number }[][]>([]);
  let canvas: HTMLCanvasElement | undefined = $state();
  let drawing = false;
  let busy = $state(false);

  function redraw() {
    const ctx = canvas?.getContext('2d');
    if (!ctx) return;
    ctx.clearRect(0, 0, W, H);
    ctx.lineWidth = 2;
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';
    ctx.strokeStyle = '#111';
    for (const s of strokes) {
      if (s.length < 2) continue;
      ctx.beginPath();
      ctx.moveTo(s[0].x, s[0].y);
      for (const p of s.slice(1)) ctx.lineTo(p.x, p.y);
      ctx.stroke();
    }
  }
  function point(e: PointerEvent) {
    const r = canvas!.getBoundingClientRect();
    return { x: ((e.clientX - r.left) * W) / r.width, y: ((e.clientY - r.top) * H) / r.height };
  }
  function down(e: PointerEvent) {
    if (e.button !== 0) return;
    drawing = true;
    canvas?.setPointerCapture(e.pointerId);
    strokes = [...strokes, [point(e)]];
  }
  function move(e: PointerEvent) {
    if (!drawing) return;
    const last = strokes[strokes.length - 1];
    strokes = [...strokes.slice(0, -1), [...last, point(e)]];
    redraw();
  }
  function up(e: PointerEvent) {
    drawing = false;
    canvas?.releasePointerCapture(e.pointerId);
    redraw();
  }
  function clear() {
    strokes = [];
    redraw();
  }
  async function chooseImage() {
    imagePath = await pickImageFile();
  }
  const canApply = $derived(
    mode === 'image' ? imagePath !== null : strokes.some((s) => s.length >= 2),
  );
  async function apply() {
    busy = true;
    const source =
      mode === 'image'
        ? ({ kind: 'image', path: imagePath! } as const)
        : ({
            kind: 'strokes',
            strokes: canvasStrokesToPdf(strokes, { w: W, h: H }, rect),
          } as const);
    const ok = await insertSignature(doc, page, rect, source);
    busy = false;
    if (ok) cancelSignature(doc);
  }
</script>

<div class="dialog-backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="sig-title">
    <h2 id="sig-title">{t.signatureTitle}</h2>
    <div class="tabs" role="tablist">
      <button
        type="button"
        role="tab"
        aria-selected={mode === 'draw'}
        class:active={mode === 'draw'}
        onclick={() => (mode = 'draw')}>{t.signatureDraw}</button
      >
      <button
        type="button"
        role="tab"
        aria-selected={mode === 'image'}
        class:active={mode === 'image'}
        onclick={() => (mode = 'image')}>{t.signatureImage}</button
      >
    </div>
    {#if mode === 'draw'}
      <canvas
        bind:this={canvas}
        width={W}
        height={H}
        aria-label={t.signatureDraw}
        onpointerdown={down}
        onpointermove={move}
        onpointerup={up}
        onpointercancel={up}
      ></canvas>
      <div class="row">
        <button type="button" onclick={clear}>{t.signatureClear}</button>
        <span class="muted">{t.signatureHint}</span>
      </div>
    {:else}
      <div class="row">
        <button type="button" onclick={chooseImage}>{t.signatureChooseImage}</button>
        <span class="muted">{imagePath ? fileName(imagePath) : t.signatureNoImage}</span>
      </div>
    {/if}
    <div class="actions">
      <button type="button" onclick={() => cancelSignature(doc)}>{t.cancel}</button>
      <button type="button" class="primary" disabled={!canApply || busy} onclick={apply}
        >{t.signatureApply}</button
      >
    </div>
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
  canvas {
    display: block;
    width: 400px;
    height: 150px;
    background: #fff;
    border: 1px dashed var(--panel-border);
    border-radius: 4px;
    touch-action: none;
    cursor: crosshair;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin: 0.5rem 0;
  }
  .muted {
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>
