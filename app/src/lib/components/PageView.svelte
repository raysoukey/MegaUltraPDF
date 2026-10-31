<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { renderFailure, type Priority } from '$lib/api';
  import { pdfRectToCss } from '$lib/logic/geometry';
  import { pageUrl, renderScale } from '$lib/pageUrl';
  import { screen } from '$lib/state/screen.svelte';
  import { type OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';
  import AnnotationLayer from './AnnotationLayer.svelte';
  import FormLayer from './FormLayer.svelte';
  import LinkLayer from './LinkLayer.svelte';
  import ObjectLayer from './ObjectLayer.svelte';
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
    pageUrl(
      doc.summary.id,
      index,
      renderScale(doc.scale, screen.dpr, page),
      doc.rotation,
      doc.generation,
      priority,
    ),
  );

  // On garde l'image précédente à l'écran (mise à l'échelle par CSS) jusqu'à ce que la
  // nouvelle soit chargée : pas de clignotement pendant un zoom.
  let shown = $state<string | null>(null);
  let loading = $state(true);
  // Message d'un rendu refusé (Lot 13, R5) : `null` tant que tout va bien.
  let failure = $state<string | null>(null);

  $effect(() => {
    const url = wanted;
    let cancelled = false;
    loading = true;
    const img = new Image();
    img.onload = () => {
      if (cancelled) return;
      shown = url;
      loading = false;
      failure = null;
    };
    img.onerror = () => {
      if (cancelled) return;
      loading = false;
      // Une balise `<img>` ne dit pas pourquoi elle a échoué : on demande la raison au
      // backend (seulement en cas d'échec). Échec passager (génération périmée, onglet
      // fermé) : `null`, on garde l'image précédente.
      renderFailure(url)
        .catch(() => '')
        .then((message) => {
          if (!cancelled) failure = message;
        });
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
  {#if failure !== null}
    <div class="failure" role="alert">
      <p>{t.renderFailed}</p>
      {#if failure}<p class="detail">{failure}</p>{/if}
    </div>
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
    <FormLayer {doc} {index} />
    <AnnotationLayer {doc} {index} />
    <ObjectLayer {doc} {index} />
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
  .failure {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    /* En haut à gauche de la page : sur une page immense, le centre serait hors de vue. */
    align-items: flex-start;
    justify-content: flex-start;
    gap: 0.5rem;
    padding: 2rem;
    text-align: start;
    color: #5a1d1d;
    background: #fdf0f0;
    border: 2px dashed #c25b5b;
  }
  .failure .detail {
    font-size: 0.85em;
    max-width: 40ch;
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
