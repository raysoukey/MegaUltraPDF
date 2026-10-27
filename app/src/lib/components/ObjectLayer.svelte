<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import type { PageObject, Rect } from '$lib/api';
  import { hitTestObjects, isContentTool, rectFromCorners } from '$lib/logic/content';
  import { cssPointToPdf, pdfRectToCss, type CssRect } from '$lib/logic/geometry';
  import type { Pt } from '$lib/logic/tools';
  import {
    ensureObjects,
    insertImageAt,
    moveObject,
    requestRedact,
    requestTextInsert,
    selectObject,
    setObjectText,
    type OpenDoc,
  } from '$lib/state/documents.svelte';
  import { toolState } from '$lib/state/tools.svelte';
  import { t } from '$lib/strings';

  let { doc, index }: { doc: OpenDoc; index: number } = $props();

  const page = $derived(doc.summary.pages[index]);
  const active = $derived(isContentTool(toolState.tool));
  let objects = $state<PageObject[]>([]);
  let root: HTMLDivElement | undefined = $state();
  let entry: HTMLTextAreaElement | undefined = $state();

  // Les objets ne sont demandés que sous un outil de contenu ; `ensureObjects` lit le cache
  // `doc.pageObjects`, vidé par l'état après chaque édition : l'effet se relance alors.
  $effect(() => {
    if (!active) return;
    let cancelled = false;
    ensureObjects(doc, index).then((list) => {
      if (!cancelled) objects = list;
    });
    return () => {
      cancelled = true;
    };
  });

  const selected = $derived.by(() => {
    const sel = doc.selectedObject;
    if (!sel || sel.page !== index) return null;
    return objects.find((o) => o.index === sel.index) ?? null;
  });

  let hover = $state<number | null>(null);
  let drag = $state<{ start: Pt; index: number; bounds: Rect } | null>(null);
  let dragDelta = $state<Pt>({ x: 0, y: 0 });
  /** Rectangle en cours de tracé (outils Image et Caviarder). */
  let draft = $state<{ from: Pt; to: Pt } | null>(null);
  /** Saisie du nouveau texte d'un objet texte. */
  let pending = $state<{ index: number; css: CssRect; value: string } | null>(null);

  $effect(() => {
    if (pending) entry?.focus();
  });

  function css(r: Rect): CssRect {
    return pdfRectToCss(r, page, doc.scale, doc.rotation);
  }
  function toPdf(e: PointerEvent | MouseEvent): Pt {
    const r = root!.getBoundingClientRect();
    return cssPointToPdf(e.clientX - r.left, e.clientY - r.top, page, doc.scale, doc.rotation);
  }
  function shifted(b: Rect): Rect {
    return { ...b, x: b.x + dragDelta.x, y: b.y + dragDelta.y };
  }

  function onPointerDown(e: PointerEvent) {
    if (e.button !== 0 || pending) return;
    const p = toPdf(e);
    switch (toolState.tool) {
      case 'objects': {
        const hit = hitTestObjects(objects, p);
        selectObject(doc, hit === null ? null : { page: index, index: hit });
        const o = hit === null ? null : objects.find((x) => x.index === hit);
        if (o) {
          drag = { start: p, index: o.index, bounds: o.bounds };
          dragDelta = { x: 0, y: 0 };
          root?.setPointerCapture(e.pointerId);
        }
        e.preventDefault();
        return;
      }
      case 'text-content':
        requestTextInsert(doc, index, [p.x, p.y]);
        e.preventDefault();
        return;
      case 'image':
      case 'redact':
        draft = { from: p, to: p };
        root?.setPointerCapture(e.pointerId);
        e.preventDefault();
        return;
    }
  }

  function onPointerMove(e: PointerEvent) {
    const p = toPdf(e);
    if (drag) {
      dragDelta = { x: p.x - drag.start.x, y: p.y - drag.start.y };
    } else if (draft) {
      draft = { from: draft.from, to: p };
    } else if (toolState.tool === 'objects') {
      hover = hitTestObjects(objects, p);
    }
  }

  function onPointerUp(e: PointerEvent) {
    root?.releasePointerCapture(e.pointerId);
    if (drag) {
      const { index: target } = drag;
      const { x: dx, y: dy } = dragDelta;
      drag = null;
      dragDelta = { x: 0, y: 0 };
      if (Math.hypot(dx, dy) >= 0.5) void moveObject(doc, index, target, dx, dy);
      return;
    }
    if (draft) {
      const r = rectFromCorners(draft.from, draft.to);
      const tool = toolState.tool;
      draft = null;
      if (r.w < 4 || r.h < 4) return;
      if (tool === 'image') void insertImageAt(doc, index, r);
      else if (tool === 'redact') requestRedact(doc, index, r);
    }
  }

  function onDoubleClick(e: MouseEvent) {
    if (toolState.tool !== 'objects' || !selected || selected.kind !== 'text') return;
    const p = toPdf(e);
    if (hitTestObjects([selected], p) === null) return;
    const b = selected.bounds;
    pending = {
      index: selected.index,
      css: css({ x: b.x, y: b.y - 40, w: Math.max(b.w, 160), h: b.h + 40 }),
      value: selected.text ?? '',
    };
  }

  function validateEntry() {
    if (!pending) return;
    const { index: target, value } = pending;
    pending = null;
    if (value.trim() !== '' && value !== selected?.text)
      void setObjectText(doc, index, target, value);
  }

  function onEntryKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      pending = null;
    } else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      e.stopPropagation();
      validateEntry();
    }
  }
</script>

{#if active}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="object-layer"
    class:pick={toolState.tool === 'objects'}
    class:redact={toolState.tool === 'redact'}
    bind:this={root}
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointercancel={onPointerUp}
    onpointerleave={() => (hover = null)}
    ondblclick={onDoubleClick}
  >
    <svg width="100%" height="100%" aria-hidden="true">
      {#if toolState.tool === 'objects'}
        {#each objects as o (o.index)}
          {@const c = css(o.index === drag?.index ? shifted(o.bounds) : o.bounds)}
          <rect
            class="object"
            class:hover={hover === o.index}
            class:selected={selected?.index === o.index}
            data-object={o.index}
            data-kind={o.kind}
            x={c.left}
            y={c.top}
            width={Math.max(c.width, 1)}
            height={Math.max(c.height, 1)}
          />
        {/each}
      {/if}
      {#if draft}
        {@const c = css(rectFromCorners(draft.from, draft.to))}
        <rect
          class="draft"
          class:redact={toolState.tool === 'redact'}
          x={c.left}
          y={c.top}
          width={c.width}
          height={c.height}
        />
      {/if}
    </svg>
    {#if pending}
      <textarea
        class="entry"
        bind:this={entry}
        bind:value={pending.value}
        style:left="{pending.css.left}px"
        style:top="{pending.css.top}px"
        style:width="{Math.max(160, pending.css.width)}px"
        style:height="{Math.max(56, pending.css.height)}px"
        placeholder={t.objectTextPlaceholder}
        aria-label={t.objectKind.text}
        onkeydown={onEntryKey}
        onblur={validateEntry}
      ></textarea>
    {/if}
  </div>
{/if}

<style>
  .object-layer {
    position: absolute;
    inset: 0;
    z-index: 3;
    pointer-events: auto;
    cursor: crosshair;
  }
  .object-layer.pick {
    cursor: default;
  }
  .object-layer svg {
    display: block;
    overflow: visible;
  }
  .object {
    fill: transparent;
    stroke: rgba(30, 136, 229, 0.35);
    stroke-width: 1;
    pointer-events: none;
  }
  .object.hover {
    fill: rgba(30, 136, 229, 0.08);
    stroke: var(--accent);
  }
  .object.selected {
    fill: rgba(30, 136, 229, 0.12);
    stroke: var(--accent);
    stroke-width: 1.5;
    stroke-dasharray: 4 3;
  }
  .draft {
    fill: rgba(30, 136, 229, 0.1);
    stroke: var(--accent);
    stroke-width: 1.5;
    stroke-dasharray: 5 3;
    pointer-events: none;
  }
  .draft.redact {
    fill: rgba(0, 0, 0, 0.55);
    stroke: #000;
  }
  .entry {
    position: absolute;
    z-index: 4;
    font: inherit;
    font-size: 0.9rem;
    resize: both;
    background: var(--panel);
    color: inherit;
    border: 1px solid var(--accent);
    border-radius: 4px;
    padding: 0.3rem;
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.25);
  }
</style>
