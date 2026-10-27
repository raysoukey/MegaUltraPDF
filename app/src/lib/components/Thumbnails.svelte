<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { pickPdfFiles, pickSavePath } from '$lib/api';
  import { rotatedSize } from '$lib/logic/geometry';
  import { clickSelection, moveTarget, shiftAfterInsert } from '$lib/logic/pageselection';
  import { pageUrl } from '$lib/pageUrl';
  import {
    applyEdit,
    extractTo,
    goToPage,
    setSelectedPages,
    type OpenDoc,
  } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';

  let { doc }: { doc: OpenDoc } = $props();

  const THUMB_WIDTH = 160;
  let list: HTMLDivElement | undefined = $state();
  let scrollTop = $state(0);
  let height = $state(0);
  // Point de départ du Maj+clic ; local au composant (pas besoin de survivre à l'onglet).
  let anchor = $state<number | null>(null);
  // Glisser-déposer : créneau d'insertion survolé (0..page_count), ou null.
  let dropSlot = $state<number | null>(null);
  let dragging = $state(false);

  const selected = $derived(new Set(doc.selectedPages));
  const count = $derived(doc.selectedPages.length);

  // Chaque vignette a la même largeur ; l'échelle dépend donc de la page.
  function scaleFor(index: number): number {
    const { w } = rotatedSize(doc.summary.pages[index], doc.rotation);
    return THUMB_WIDTH / w;
  }
  function heightFor(index: number): number {
    const { h } = rotatedSize(doc.summary.pages[index], doc.rotation);
    return h * scaleFor(index);
  }
  const ROW_GAP = 28; // libellé + marge
  const tops = $derived.by(() => {
    const result: number[] = [];
    let y = 0;
    for (let i = 0; i < doc.summary.page_count; i++) {
      result.push(y);
      y += heightFor(i) + ROW_GAP;
    }
    return result;
  });
  const total = $derived(
    tops.length ? tops[tops.length - 1] + heightFor(tops.length - 1) + ROW_GAP : 0,
  );
  const visible = $derived.by(() => {
    const out: number[] = [];
    for (let i = 0; i < tops.length; i++) {
      if (tops[i] + heightFor(i) >= scrollTop - 400 && tops[i] <= scrollTop + height + 400)
        out.push(i);
    }
    return out;
  });
  /** Position verticale de l'indicateur d'insertion pour un créneau. */
  function slotTop(slot: number): number {
    if (slot >= tops.length) return total - ROW_GAP / 2;
    return tops[slot] - ROW_GAP / 2;
  }

  $effect(() => {
    if (!list) return;
    const ro = new ResizeObserver((e) => (height = e[0]?.contentRect.height ?? 0));
    ro.observe(list);
    return () => ro.disconnect();
  });

  // Garde la vignette de la page courante visible.
  $effect(() => {
    const top = tops[doc.page];
    if (!list || top === undefined) return;
    if (top < list.scrollTop || top + heightFor(doc.page) > list.scrollTop + list.clientHeight) {
      list.scrollTop = Math.max(0, top - 20);
    }
  });

  function onClick(e: MouseEvent, i: number) {
    const next = clickSelection(doc.selectedPages, anchor, i, {
      ctrl: e.ctrlKey || e.metaKey,
      shift: e.shiftKey,
    });
    anchor = next.anchor;
    setSelectedPages(doc, next.selected);
    if (!e.ctrlKey && !e.metaKey && !e.shiftKey) goToPage(doc, i);
  }

  /** Pages visées par une action : la sélection, sinon la page courante. */
  function targets(): number[] {
    return count > 0 ? [...doc.selectedPages] : [doc.page];
  }

  function rotate(quarterTurns: number) {
    applyEdit(doc, { kind: 'rotate_pages', pages: targets(), quarter_turns: quarterTurns });
  }
  async function remove() {
    const ok = await applyEdit(doc, { kind: 'delete_pages', pages: targets() });
    if (ok) setSelectedPages(doc, []);
  }
  async function insertBlank(after: boolean) {
    const ref = count > 0 ? Math.max(...doc.selectedPages) : doc.page;
    const at = after ? ref + 1 : Math.min(...targets());
    const selected = [...doc.selectedPages];
    // Même format que la page de référence.
    const ok = await applyEdit(doc, {
      kind: 'insert_blank_page',
      at,
      size: doc.summary.pages[ref],
    });
    // La sélection suit les pages qu'elle visait (elles ont reculé d'un cran).
    if (ok) setSelectedPages(doc, shiftAfterInsert(selected, at, 1));
  }
  async function insertFromFile() {
    const paths = await pickPdfFiles();
    let at = (count > 0 ? Math.max(...doc.selectedPages) : doc.page) + 1;
    let selected = [...doc.selectedPages];
    for (const path of paths) {
      const before = doc.summary.page_count;
      const ok = await applyEdit(doc, { kind: 'insert_pages_from_file', path, pages: null, at });
      if (!ok) break;
      const inserted = doc.summary.page_count - before;
      selected = shiftAfterInsert(selected, at, inserted);
      at += inserted;
    }
    setSelectedPages(doc, selected);
  }
  async function extract() {
    const pages = targets();
    const path = await pickSavePath();
    if (!path) return;
    await extractTo(doc, pages, path);
  }

  // ---- Glisser-déposer HTML5 ----
  function onDragStart(e: DragEvent, i: number) {
    if (!selected.has(i)) {
      anchor = i;
      setSelectedPages(doc, [i]);
    }
    dragging = true;
    e.dataTransfer?.setData('text/plain', String(i));
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
  }
  function onDragOver(e: DragEvent, i: number) {
    if (!dragging) return;
    e.preventDefault();
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const after = e.clientY - rect.top > rect.height / 2;
    dropSlot = after ? i + 1 : i;
  }
  function onDrop(e: DragEvent) {
    e.preventDefault();
    const slot = dropSlot;
    endDrag();
    if (slot === null) return;
    const to = moveTarget(doc.summary.page_count, doc.selectedPages, slot);
    if (to === null) return;
    const pages = [...doc.selectedPages];
    applyEdit(doc, { kind: 'move_pages', pages, to }).then((ok) => {
      // Les pages déplacées se suivent désormais à partir de `to`.
      if (ok)
        setSelectedPages(
          doc,
          pages.map((_, k) => to + k),
        );
    });
  }
  function endDrag() {
    dragging = false;
    dropSlot = null;
  }
</script>

<div class="panel">
  <div class="actions" role="toolbar" aria-label={t.pageActions}>
    <span class="count">{count > 0 ? t.selectedPages(count) : t.currentPageTarget}</span>
    <button aria-label={t.rotateLeft} type="button" title={t.rotateLeft} onclick={() => rotate(-1)}
      >↺</button
    >
    <button aria-label={t.rotateRight} type="button" title={t.rotateRight} onclick={() => rotate(1)}
      >↻</button
    >
    <button aria-label={t.deletePages} type="button" title={t.deletePages} onclick={remove}
      >🗑</button
    >
    <button
      aria-label={t.insertBlankBefore}
      type="button"
      title={t.insertBlankBefore}
      onclick={() => insertBlank(false)}>⬆︎▭</button
    >
    <button
      aria-label={t.insertBlankAfter}
      type="button"
      title={t.insertBlankAfter}
      onclick={() => insertBlank(true)}>▭⬇︎</button
    >
    <button
      aria-label={t.insertFromFile}
      type="button"
      title={t.insertFromFile}
      onclick={insertFromFile}>📄+</button
    >
    <button aria-label={t.extractPages} type="button" title={t.extractPages} onclick={extract}
      >⤓</button
    >
  </div>
  <div
    class="list"
    bind:this={list}
    role="listbox"
    aria-multiselectable="true"
    aria-label={t.thumbnails}
    tabindex="-1"
    onscroll={() => (scrollTop = list?.scrollTop ?? 0)}
    ondragover={(e) => {
      if (!dragging) return;
      e.preventDefault();
      // Sous la dernière vignette : à la fin.
      if (e.target === list || e.target === list?.firstElementChild)
        dropSlot = doc.summary.page_count;
    }}
    ondrop={onDrop}
  >
    <div class="spacer" style:height="{total}px">
      {#each visible as i (i)}
        <button
          type="button"
          class="thumb"
          class:current={i === doc.page}
          class:selected={selected.has(i)}
          style:top="{tops[i]}px"
          style:width="{THUMB_WIDTH}px"
          role="option"
          aria-selected={selected.has(i)}
          aria-label={t.pageOf(i + 1, doc.summary.page_count)}
          aria-current={i === doc.page ? 'page' : undefined}
          draggable="true"
          onclick={(e) => onClick(e, i)}
          ondragstart={(e) => onDragStart(e, i)}
          ondragover={(e) => onDragOver(e, i)}
          ondragend={endDrag}
        >
          <img
            src={pageUrl(doc.summary.id, i, scaleFor(i), doc.rotation, doc.generation, 'thumbnail')}
            alt=""
            width={THUMB_WIDTH}
            height={heightFor(i)}
            loading="lazy"
            draggable="false"
          />
          <span>{i + 1}</span>
        </button>
      {/each}
      {#if dropSlot !== null}
        <div class="drop-indicator" style:top="{slotTop(dropSlot)}px" aria-hidden="true"></div>
      {/if}
    </div>
  </div>
</div>

<style>
  .panel {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
    padding: 4px 6px;
    border-bottom: 1px solid var(--panel-border);
    align-items: center;
  }
  .actions .count {
    width: 100%;
    font-size: 0.75rem;
    color: var(--muted);
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 12px 0;
    box-sizing: border-box;
  }
  .spacer {
    position: relative;
  }
  .thumb {
    position: absolute;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 4px;
    border: 2px solid transparent;
    border-radius: 6px;
    cursor: grab;
  }
  .thumb.current {
    border-color: var(--accent);
  }
  .thumb.selected {
    background: var(--accent-soft);
  }
  .thumb img {
    display: block;
    background: #fff;
    box-shadow: var(--page-shadow);
  }
  .thumb span {
    font-size: 0.8rem;
    color: var(--muted);
  }
  .drop-indicator {
    position: absolute;
    left: 12px;
    right: 12px;
    height: 3px;
    background: var(--accent);
    border-radius: 2px;
    pointer-events: none;
  }
</style>
