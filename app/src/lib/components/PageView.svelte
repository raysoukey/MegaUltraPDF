<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import type { Priority } from '$lib/api';
  import { pdfRectToCss } from '$lib/logic/geometry';
  import { pageUrl } from '$lib/pageUrl';
  import { type OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';
  import AnnotationLayer from './AnnotationLayer.svelte';
  import LinkLayer from './LinkLayer.svelte';
  import TextLayer from './TextLayer.svelte';

  let {
    doc,
    index,
    width,
    height,
    priority,
    interactive = true,
    onnavigate,
  }: {
    doc: OpenDoc;
    index: number;
    width: number;
    height: number;
    priority: Priority;
    interactive?: boolean;
    onnavigate?: (page: number) => void;
  } = $props();

  const page = $derived(doc.summary.pages[index]);
  // L'URL demandée change avec l'échelle, la rotation et la génération.
  const wanted = $derived(
    pageUrl(doc.summary.id, index, doc.scale, doc.rotation, doc.generation, priority),
  );

  // On garde l'image précédente à l'écran (mise à l'échelle par CSS) jusqu'à ce que la
  // nouvelle soit chargée : pas de clignotement pendant un zoom.
  let shown = $state<string | null>(null);
  let loading = $state(true);

  $effect(() => {
    const url = wanted;
    let cancelled = false;
    loading = true;
    const img = new Image();
    img.onload = () => {
      if (cancelled) return;
      shown = url;
      loading = false;
    };
    img.onerror = () => {
      if (!cancelled) loading = false; // 409 (génération périmée) ou autre : on garde l'ancienne
    };
    // Petit délai avant de demander l'image : pendant un défilement rapide, les pages
    // simplement traversées sont démontées avant l'échéance et ne coûtent aucun rendu.
    const timer = setTimeout(() => {
      if (!cancelled) img.src = url;
    }, 80);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  });

  const searchRects = $derived.by(() => {
    if (!doc.search) return [];
    return doc.search.hits.flatMap((hit, i) =>
      hit.page === index
        ? hit.rects.map((r) => ({
            css: pdfRectToCss(r, page, doc.scale, doc.rotation),
            current: i === doc.search?.current,
          }))
        : [],
    );
  });
</script>

<div
  class="page"
  class:loading
  style:width="{width}px"
  style:height="{height}px"
  data-page={index}
  aria-label={t.pageOf(index + 1, doc.summary.page_count)}
>
  {#if shown}
    <img src={shown} alt={t.pageOf(index + 1, doc.summary.page_count)} draggable="false" />
  {/if}
  {#each searchRects as r, i (i)}
    <div
      class="hit"
      class:current={r.current}
      style:left="{r.css.left}px"
      style:top="{r.css.top}px"
      style:width="{r.css.width}px"
      style:height="{r.css.height}px"
    ></div>
  {/each}
  {#if interactive}
    <LinkLayer {doc} {index} {onnavigate} />
    <TextLayer {doc} {index} />
    <AnnotationLayer {doc} {index} />
  {/if}
</div>

<style>
  .page {
    position: relative;
    background: #fff;
    box-shadow: var(--page-shadow);
    overflow: hidden;
    user-select: none;
  }
  .page.loading::after {
    content: '';
    position: absolute;
    inset: 0;
    background: repeating-linear-gradient(45deg, transparent 0 14px, rgba(0, 0, 0, 0.03) 14px 28px);
    pointer-events: none;
  }
  img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    display: block;
  }
  .hit {
    position: absolute;
    background: var(--highlight);
    pointer-events: none;
  }
  .hit.current {
    background: var(--highlight-current);
    outline: 1px solid rgba(0, 0, 0, 0.3);
  }
</style>
