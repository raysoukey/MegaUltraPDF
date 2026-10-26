<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { rotatedSize } from '$lib/logic/geometry';
  import { pageUrl } from '$lib/pageUrl';
  import { goToPage, type OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';

  let { doc }: { doc: OpenDoc } = $props();

  const THUMB_WIDTH = 160;
  let list: HTMLDivElement | undefined = $state();
  let scrollTop = $state(0);
  let height = $state(0);

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
</script>

<div class="list" bind:this={list} onscroll={() => (scrollTop = list?.scrollTop ?? 0)}>
  <div class="spacer" style:height="{total}px">
    {#each visible as i (i)}
      <button
        type="button"
        class="thumb"
        class:current={i === doc.page}
        style:top="{tops[i]}px"
        style:width="{THUMB_WIDTH}px"
        aria-label={t.pageOf(i + 1, doc.summary.page_count)}
        aria-current={i === doc.page ? 'page' : undefined}
        onclick={() => goToPage(doc, i)}
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
  </div>
</div>

<style>
  .list {
    height: 100%;
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
  }
  .thumb.current {
    border-color: var(--accent);
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
</style>
