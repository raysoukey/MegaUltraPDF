<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { fitPageScale, fitWidthScale, nextZoom } from '$lib/logic/zoom';
  import { pageAtScroll, pageLayout, visiblePages } from '$lib/logic/virtualization';
  import {
    consumeScrollRequest,
    goToPage,
    notePage,
    setScale,
    type OpenDoc,
  } from '$lib/state/documents.svelte';
  import { settings } from '$lib/state/settings.svelte';
  import PageView from './PageView.svelte';

  let { doc }: { doc: OpenDoc } = $props();

  // Une visionneuse neuve (changement d'onglet, remontage au changement de langue) reprend à
  // la page courante du document au lieu du haut du fichier.
  // [Apprentissage] Pas de `$effect` ici : un effet relit toutes les valeurs réactives qu'il
  // touche (`doc.page` change à chaque défilement) et se relancerait sans cesse. `onMount`
  // ne s'exécute qu'une fois, à la création du composant.
  onMount(() => goToPage(doc, doc.page));

  const GAP = 16;
  const GUTTER = 24;

  let container: HTMLDivElement | undefined = $state();
  let viewportWidth = $state(0);
  let viewportHeight = $state(0);
  let scrollTop = $state(0);

  const single = $derived(settings.viewMode === 'single');
  const pagesForLayout = $derived(single ? [doc.summary.pages[doc.page]] : doc.summary.pages);
  const layout = $derived(pageLayout(pagesForLayout, doc.scale, doc.rotation, GAP));
  const totalHeight = $derived(
    layout.length ? layout[layout.length - 1].top + layout[layout.length - 1].height : 0,
  );
  const maxWidth = $derived(Math.max(0, ...layout.map((s) => s.width)));
  const visible = $derived(single ? [0] : visiblePages(layout, scrollTop, viewportHeight, 1));
  const prefetch = $derived(
    single
      ? []
      : visiblePages(layout, scrollTop, viewportHeight, 3).filter((i) => !visible.includes(i)),
  );

  function realIndex(layoutIndex: number): number {
    return single ? doc.page : layoutIndex;
  }

  /** Échelle « ajustée » pour la page courante selon le mode. */
  export function fitWidth() {
    const page = doc.summary.pages[doc.page];
    if (!page || viewportWidth === 0) return;
    const w =
      doc.rotation === 'cw90' || doc.rotation === 'cw270'
        ? { width_pt: page.height_pt, height_pt: page.width_pt }
        : page;
    setScale(doc, fitWidthScale(w, viewportWidth, GUTTER), 'fitWidth');
  }

  export function fitPage() {
    const page = doc.summary.pages[doc.page];
    if (!page || viewportWidth === 0) return;
    const w =
      doc.rotation === 'cw90' || doc.rotation === 'cw270'
        ? { width_pt: page.height_pt, height_pt: page.width_pt }
        : page;
    setScale(doc, fitPageScale(w, viewportWidth, viewportHeight, GUTTER), 'fitPage');
  }

  // Recalcule les modes « ajuster » quand la fenêtre ou la rotation changent.
  $effect(() => {
    void viewportWidth;
    void viewportHeight;
    void doc.rotation;
    if (doc.zoomMode === 'fitWidth') fitWidth();
    else if (doc.zoomMode === 'fitPage') fitPage();
  });

  $effect(() => {
    if (!container) return;
    const ro = new ResizeObserver((entries) => {
      const r = entries[0]?.contentRect;
      if (r) {
        viewportWidth = r.width;
        viewportHeight = r.height;
      }
    });
    ro.observe(container);
    return () => ro.disconnect();
  });

  // Défilement demandé (navigation, signet, recherche, ouverture).
  $effect(() => {
    const target = doc.scrollRequest;
    if (target === null || !container) return;
    consumeScrollRequest(doc);
    if (single) {
      container.scrollTop = 0;
      return;
    }
    const slot = layout[target];
    if (slot) container.scrollTop = slot.top;
  });

  // Zoom : on garde la page courante à la même position relative.
  // svelte-ignore state_referenced_locally
  let lastScale = doc.scale;
  $effect(() => {
    const scale = doc.scale;
    if (!container || scale === lastScale) return;
    const ratio = scale / lastScale;
    lastScale = scale;
    container.scrollTop = container.scrollTop * ratio;
  });

  function onScroll() {
    if (!container) return;
    scrollTop = container.scrollTop;
    if (!single) {
      const current = pageAtScroll(layout, scrollTop, viewportHeight);
      if (current !== doc.page) notePage(doc, current);
    }
  }

  function onWheel(e: WheelEvent) {
    if (!e.ctrlKey) return;
    e.preventDefault();
    setScale(doc, nextZoom(doc.scale, e.deltaY < 0 ? 1 : -1), 'custom');
  }
</script>

<div class="viewer" bind:this={container} onscroll={onScroll} onwheel={onWheel} tabindex="-1">
  <div class="stack" style:height="{totalHeight}px" style:min-width="{maxWidth + 2 * GUTTER}px">
    {#each visible as li (realIndex(li))}
      {@const slot = layout[li]}
      <div
        class="slot"
        style:top="{slot.top}px"
        style:width="{slot.width}px"
        style:height="{slot.height}px"
      >
        <PageView
          {doc}
          index={realIndex(li)}
          width={slot.width}
          height={slot.height}
          priority="visible"
          onnavigate={(p) => goToPage(doc, p)}
        />
      </div>
    {/each}
    {#each prefetch as li (`p${li}`)}
      {@const slot = layout[li]}
      <div
        class="slot"
        style:top="{slot.top}px"
        style:width="{slot.width}px"
        style:height="{slot.height}px"
      >
        <PageView
          {doc}
          index={li}
          width={slot.width}
          height={slot.height}
          priority="prefetch"
          interactive={false}
        />
      </div>
    {/each}
  </div>
</div>

<style>
  .viewer {
    position: relative;
    flex: 1;
    overflow: auto;
    outline: none;
    background: var(--bg);
  }
  .stack {
    position: relative;
    margin: 0 auto;
    padding: 16px 0;
    width: max-content;
  }
  .slot {
    position: absolute;
    left: 50%;
    transform: translateX(-50%);
  }
</style>
