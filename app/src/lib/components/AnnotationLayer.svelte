<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import type { Annotation, Rect } from '$lib/api';
  import { handlesFor, hitTest, noteIconRect, resize, type HandleId } from '$lib/logic/annotations';
  import { cssPointToPdf, pdfRectToCss, type CssRect } from '$lib/logic/geometry';
  import { isContentTool, rectFromCorners } from '$lib/logic/content';
  import {
    beginDraft,
    extendDraft,
    finishDraft,
    isDrawingTool,
    stampLabel,
    type Draft,
    type Pt,
  } from '$lib/logic/tools';
  import {
    addAnnotation,
    ensureAnnotations,
    moveAnnotation,
    requestSignature,
    selectAnnotation,
    updateAnnotation,
    type OpenDoc,
  } from '$lib/state/documents.svelte';
  import { settings } from '$lib/state/settings.svelte';
  import { currentStyle, setTool, toolState } from '$lib/state/tools.svelte';
  import { t } from '$lib/strings';

  let { doc, index }: { doc: OpenDoc; index: number } = $props();

  const page = $derived(doc.summary.pages[index]);
  let annotations = $state<Annotation[]>([]);
  let root: HTMLDivElement | undefined = $state();
  let entry: HTMLTextAreaElement | undefined = $state();

  // Rechargé après chaque édition : `ensureAnnotations` lit `doc.annotations`, vidé par l'état.
  $effect(() => {
    let cancelled = false;
    ensureAnnotations(doc, index).then((list) => {
      if (!cancelled) annotations = list;
    });
    return () => {
      cancelled = true;
    };
  });

  const drawing = $derived(isDrawingTool(toolState.tool));
  // Sous un outil de contenu, le calque d'annotation laisse passer tous les gestes.
  const passive = $derived(isContentTool(toolState.tool));
  const selected = $derived.by(() => {
    const sel = doc.selectedAnnotation;
    if (!sel || sel.page !== index) return null;
    return annotations.find((a) => a.index === sel.index) ?? null;
  });

  let draft = $state<Draft | null>(null);
  let drag = $state<{ handle: HandleId; start: Pt; original: Rect; index: number } | null>(null);
  let dragPreview = $state<Rect | null>(null);
  /** Saisie de texte en cours : nouveau brouillon (note, texte) ou annotation à modifier. */
  let pending = $state<{
    draft: Draft | null;
    editing: Annotation | null;
    css: CssRect;
    value: string;
  } | null>(null);

  $effect(() => {
    if (pending) entry?.focus();
  });

  function css(r: Rect): CssRect {
    return pdfRectToCss(r, page, doc.scale, doc.rotation);
  }
  function cssPoint(p: Pt): { x: number; y: number } {
    const c = css({ x: p.x, y: p.y, w: 0, h: 0 });
    return { x: c.left, y: c.top };
  }
  function toPdf(e: PointerEvent): Pt {
    const r = root!.getBoundingClientRect();
    return cssPointToPdf(e.clientX - r.left, e.clientY - r.top, page, doc.scale, doc.rotation);
  }
  function isMarkup(a: Annotation): boolean {
    return (
      a.kind === 'highlight' ||
      a.kind === 'underline' ||
      a.kind === 'strike_out' ||
      a.kind === 'squiggly'
    );
  }
  function grabRect(a: Annotation): Rect {
    return a.kind === 'note' ? noteIconRect(a) : a.rect;
  }
  /** Déplaçable : décidé en Rust (`editable`), hors marquages de texte qui suivent le texte. */
  function movable(a: Annotation): boolean {
    return a.editable && !isMarkup(a);
  }
  function draftRect(d: Draft): Rect {
    const [a, b = a] = d.points;
    return rectFromCorners(a, b);
  }

  function commit(d: Draft, contents: string) {
    const a = finishDraft(d, currentStyle(d.tool), contents, settings.author);
    if (a) void addAnnotation(doc, index, a);
  }

  function openEntry(d: Draft | null, editing: Annotation | null) {
    const rect = editing
      ? editing.kind === 'note'
        ? { ...noteIconRect(editing), y: noteIconRect(editing).y - 60, h: 60, w: 180 }
        : editing.rect
      : d && d.tool === 'note'
        ? { x: d.points[0].x, y: d.points[0].y - 60, w: 180, h: 60 }
        : draftRect(d!);
    pending = { draft: d, editing, css: css(rect), value: editing?.contents ?? '' };
  }

  function validateEntry() {
    if (!pending) return;
    const { draft: d, editing, value } = pending;
    pending = null;
    if (editing) {
      void updateAnnotation(doc, index, editing.index, { contents: value });
    } else if (d) {
      commit(d, value);
    }
  }

  function onPointerDown(e: PointerEvent) {
    if (e.button !== 0 || pending) return;
    const p = toPdf(e);
    const tool = toolState.tool;
    if (drawing) {
      const d = beginDraft(tool, p);
      if (!d) return;
      if (tool === 'stamp') {
        commit(d, stampLabel(toolState.stamp));
        return;
      }
      // Note et texte : la saisie s'ouvre au relâchement (un champ ouvert pendant le
      // geste perdrait le focus, donc se validerait, avant la première frappe).
      draft = d;
      root?.setPointerCapture(e.pointerId);
      e.preventDefault();
      return;
    }
    // Outil Sélectionner : poignée, puis annotation sous le pointeur.
    const handleEl = (e.target as Element).closest('[data-handle]');
    if (handleEl && selected) {
      drag = {
        handle: handleEl.getAttribute('data-handle') as HandleId,
        start: p,
        original: grabRect(selected),
        index: selected.index,
      };
      root?.setPointerCapture(e.pointerId);
      e.preventDefault();
      return;
    }
    const hit = hitTest(
      annotations.filter((a) => !isMarkup(a)),
      p,
    );
    if (hit === null) {
      if (selected) selectAnnotation(doc, null);
      return;
    }
    selectAnnotation(doc, { page: index, index: hit });
    const a = annotations.find((x) => x.index === hit);
    if (a && movable(a)) {
      drag = { handle: 'move', start: p, original: grabRect(a), index: hit };
      root?.setPointerCapture(e.pointerId);
    }
    e.preventDefault();
  }

  function onPointerMove(e: PointerEvent) {
    if (draft) {
      draft = extendDraft(draft, toPdf(e));
    } else if (drag) {
      const p = toPdf(e);
      dragPreview = resize(drag.original, drag.handle, {
        dx: p.x - drag.start.x,
        dy: p.y - drag.start.y,
      });
    }
  }

  function onPointerUp(e: PointerEvent) {
    root?.releasePointerCapture(e.pointerId);
    if (draft) {
      const d = draft;
      draft = null;
      if (d.tool === 'text' || d.tool === 'note') openEntry(d, null);
      else if (d.tool === 'signature') {
        const r = draftRect(d);
        if (r.w >= 10 && r.h >= 5) {
          requestSignature(doc, index, r);
          setTool('select');
        }
      } else commit(d, '');
      return;
    }
    if (drag) {
      const { index: target, original } = drag;
      const final = dragPreview;
      drag = null;
      dragPreview = null;
      if (
        final &&
        (Math.abs(final.x - original.x) > 0.5 ||
          Math.abs(final.y - original.y) > 0.5 ||
          Math.abs(final.w - original.w) > 0.5 ||
          Math.abs(final.h - original.h) > 0.5)
      ) {
        void moveAnnotation(doc, index, target, final);
      }
    }
  }

  function onDoubleClick(e: MouseEvent) {
    if (!selected || !root) return;
    const r = root.getBoundingClientRect();
    const p = cssPointToPdf(e.clientX - r.left, e.clientY - r.top, page, doc.scale, doc.rotation);
    const inside =
      p.x >= grabRect(selected).x &&
      p.x <= grabRect(selected).x + grabRect(selected).w &&
      p.y >= grabRect(selected).y &&
      p.y <= grabRect(selected).y + grabRect(selected).h;
    if (
      inside &&
      (selected.kind === 'note' || selected.kind === 'text' || selected.kind === 'stamp')
    )
      openEntry(null, selected);
  }

  function onEntryKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      pending = null;
      setTool('select');
    } else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      e.stopPropagation();
      validateEntry();
    }
  }

  function rgb(c: { r: number; g: number; b: number } | null, fallback = '#f5b400'): string {
    return c ? `rgb(${c.r}, ${c.g}, ${c.b})` : fallback;
  }
  const previewColor = $derived(rgb(currentStyle().color));
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="annotation-layer"
  class:drawing
  class:selecting={!drawing && !passive}
  bind:this={root}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
  ondblclick={onDoubleClick}
>
  <svg width="100%" height="100%" aria-hidden="true">
    {#each annotations as a (a.index)}
      {#if a.kind === 'note'}
        {@const c = css(noteIconRect(a))}
        <!-- Le symbole lui-même est dessiné par PDFium dans l'image de la page ; ici,
             seulement la zone de saisie (repère pour le clic et les tests). -->
        <rect class="note" x={c.left} y={c.top} width={c.width} height={c.height} />
      {/if}
      {#if movable(a)}
        {@const c = css(grabRect(a))}
        <rect class="hit" x={c.left} y={c.top} width={c.width} height={c.height} />
      {/if}
    {/each}
    {#if selected}
      {@const c = css(dragPreview ?? grabRect(selected))}
      <rect class="outline" x={c.left} y={c.top} width={c.width} height={c.height} />
      {#if movable(selected) && selected.kind !== 'note'}
        {#each handlesFor(dragPreview ?? grabRect(selected)).filter((h) => h.id !== 'move') as h (h.id)}
          {@const hp = cssPoint(h.at)}
          <rect class="handle" data-handle={h.id} x={hp.x - 4} y={hp.y - 4} width="8" height="8" />
        {/each}
      {/if}
    {/if}
    {#if draft}
      {@const d = draft}
      {#if d.tool === 'rect' || d.tool === 'ellipse' || d.tool === 'text' || d.tool === 'signature'}
        {@const c = css(draftRect(d))}
        {#if d.tool === 'ellipse'}
          <ellipse
            class="draft"
            cx={c.left + c.width / 2}
            cy={c.top + c.height / 2}
            rx={c.width / 2}
            ry={c.height / 2}
            stroke={previewColor}
          />
        {:else}
          <rect
            class="draft"
            x={c.left}
            y={c.top}
            width={c.width}
            height={c.height}
            stroke={previewColor}
          />
        {/if}
      {:else if (d.tool === 'line' || d.tool === 'arrow') && d.points[1]}
        {@const a = cssPoint(d.points[0])}
        {@const b = cssPoint(d.points[1])}
        <line class="draft" x1={a.x} y1={a.y} x2={b.x} y2={b.y} stroke={previewColor} />
      {:else if d.tool === 'ink'}
        <polyline
          class="draft"
          points={d.points
            .map((p) => {
              const c = cssPoint(p);
              return `${c.x},${c.y}`;
            })
            .join(' ')}
          stroke={previewColor}
        />
      {/if}
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
      placeholder={t.annotationContentsPlaceholder}
      aria-label={t.annotationContents}
      onkeydown={onEntryKey}
      onblur={validateEntry}
    ></textarea>
  {/if}
</div>

<style>
  .annotation-layer {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  .annotation-layer.drawing {
    pointer-events: auto;
    cursor: crosshair;
    z-index: 3;
  }
  .annotation-layer svg {
    display: block;
    overflow: visible;
  }
  .selecting .hit {
    pointer-events: auto;
    fill: transparent;
    cursor: move;
  }
  .drawing .hit {
    pointer-events: none;
    fill: transparent;
  }
  .note {
    fill: transparent;
    pointer-events: none;
  }
  .outline {
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.5;
    stroke-dasharray: 4 3;
    pointer-events: none;
  }
  .handle {
    fill: #fff;
    stroke: var(--accent);
    stroke-width: 1.5;
    pointer-events: auto;
    cursor: nwse-resize;
  }
  .draft {
    fill: none;
    stroke-width: 2;
    stroke-dasharray: 5 3;
    pointer-events: none;
  }
  .entry {
    position: absolute;
    z-index: 4;
    pointer-events: auto;
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
