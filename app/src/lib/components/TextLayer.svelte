<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import type { TextBox } from '$lib/api';
  import { cssPointToPdf, pdfRectToCss } from '$lib/logic/geometry';
  import { charIndexAt, selectionRects } from '$lib/logic/selection';
  import { ensureTextLayer, setSelection, type OpenDoc } from '$lib/state/documents.svelte';

  let { doc, index }: { doc: OpenDoc; index: number } = $props();

  const page = $derived(doc.summary.pages[index]);
  let boxes = $state<TextBox[]>([]);
  let dragging = $state(false);
  let layer: HTMLDivElement | undefined = $state();

  $effect(() => {
    let cancelled = false;
    ensureTextLayer(doc, index).then((b) => {
      if (!cancelled) boxes = b;
    });
    return () => {
      cancelled = true;
    };
  });

  function pdfPoint(e: PointerEvent): { x: number; y: number } | null {
    if (!layer) return null;
    const r = layer.getBoundingClientRect();
    return cssPointToPdf(e.clientX - r.left, e.clientY - r.top, page, doc.scale, doc.rotation);
  }

  function onPointerDown(e: PointerEvent) {
    if (e.button !== 0 || boxes.length === 0) return;
    const p = pdfPoint(e);
    if (!p) return;
    const i = charIndexAt(boxes, p.x, p.y);
    if (i < 0) return;
    dragging = true;
    layer?.setPointerCapture(e.pointerId);
    setSelection(doc, { page: index, from: i, to: i });
    e.preventDefault();
  }

  function onPointerMove(e: PointerEvent) {
    if (!dragging || !doc.selection) return;
    const p = pdfPoint(e);
    if (!p) return;
    const i = charIndexAt(boxes, p.x, p.y);
    if (i >= 0) setSelection(doc, { ...doc.selection, to: i });
  }

  function onPointerUp(e: PointerEvent) {
    if (!dragging) return;
    dragging = false;
    layer?.releasePointerCapture(e.pointerId);
    // Un simple clic sans glisser désélectionne.
    if (doc.selection && doc.selection.from === doc.selection.to && e.detail < 2) {
      setSelection(doc, null);
    }
  }

  function onDoubleClick(e: MouseEvent) {
    const r = layer?.getBoundingClientRect();
    if (!r || boxes.length === 0) return;
    const p = cssPointToPdf(e.clientX - r.left, e.clientY - r.top, page, doc.scale, doc.rotation);
    const i = charIndexAt(boxes, p.x, p.y);
    if (i < 0) return;
    // Mot : on s'étend tant que les caractères ne sont pas des espaces.
    const isWord = (c: string) => !/\s/.test(c);
    let from = i;
    let to = i;
    while (from > 0 && isWord(boxes[from - 1].ch) && isWord(boxes[from].ch)) from--;
    while (to < boxes.length - 1 && isWord(boxes[to + 1].ch) && isWord(boxes[to].ch)) to++;
    setSelection(doc, { page: index, from, to });
  }

  const rects = $derived.by(() => {
    const sel = doc.selection;
    if (!sel || sel.page !== index || boxes.length === 0) return [];
    return selectionRects(boxes, sel.from, sel.to).map((r) =>
      pdfRectToCss(r, page, doc.scale, doc.rotation),
    );
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="text-layer"
  class:has-text={boxes.length > 0}
  bind:this={layer}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
  ondblclick={onDoubleClick}
>
  {#each rects as r, i (i)}
    <div
      class="sel"
      style:left="{r.left}px"
      style:top="{r.top}px"
      style:width="{r.width}px"
      style:height="{r.height}px"
    ></div>
  {/each}
</div>

<style>
  .text-layer {
    position: absolute;
    inset: 0;
  }
  .text-layer.has-text {
    cursor: text;
  }
  .sel {
    position: absolute;
    background: var(--selection);
    pointer-events: none;
  }
</style>
