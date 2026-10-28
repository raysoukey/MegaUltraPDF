<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { trapFocus } from '$lib/actions/trapFocus';
  import { scopePages, type PageScope } from '$lib/logic/content';
  import { printScale } from '$lib/logic/export';
  import { pageUrl } from '$lib/pageUrl';
  import type { OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';

  let { doc, onclose }: { doc: OpenDoc; onclose: () => void } = $props();

  let scope = $state<PageScope>('all');
  let dpi = $state(150);
  // URLs déjà chargées : un compteur se tromperait quand une image est réutilisée par
  // Svelte (même URL) après un changement de portée ; un ensemble, non.
  let loaded = $state<string[]>([]);
  let printButton: HTMLButtonElement | undefined = $state();

  $effect(() => {
    printButton?.focus();
  });

  const pages = $derived(
    scopePages(scope, doc.page, doc.selectedPages) ??
      Array.from({ length: doc.summary.page_count }, (_, i) => i),
  );
  // Les images passent par le même protocole que la visionneuse, à l'échelle d'impression.
  const urls = $derived(
    pages.map((p) =>
      pageUrl(doc.summary.id, p, printScale(dpi), 'none', doc.generation, 'visible'),
    ),
  );
  const ready = $derived(urls.every((u) => loaded.includes(u)));
  const loadedCount = $derived(urls.filter((u) => loaded.includes(u)).length);
  function markLoaded(url: string) {
    if (!loaded.includes(url)) loaded = [...loaded, url];
  }

  function print() {
    window.print();
  }
</script>

<div class="print-view" role="dialog" aria-modal="true" use:trapFocus aria-labelledby="print-title">
  <header class="print-bar">
    <h2 id="print-title">{t.printTitle}</h2>
    <label>
      {t.pageScope}
      <select bind:value={scope}>
        <option value="all">{t.scopeAll}</option>
        <option value="current">{t.scopeCurrent(doc.page + 1)}</option>
        <option value="selected" disabled={doc.selectedPages.length === 0}
          >{t.scopeSelected(doc.selectedPages.length)}</option
        >
      </select>
    </label>
    <label>
      {t.printResolution}
      <select bind:value={dpi}>
        <option value={150}>{t.dpiValue(150)}</option>
        <option value={300}>{t.dpiValue(300)}</option>
      </select>
    </label>
    <span class="muted" role="status">
      {ready ? t.printReady(pages.length) : t.printLoading(loadedCount, pages.length)}
    </span>
    <span class="spacer"></span>
    <button type="button" onclick={onclose}>{t.close}</button>
    <button type="button" class="primary" bind:this={printButton} disabled={!ready} onclick={print}
      >{t.printAction}</button
    >
  </header>
  <section class="sheets">
    {#each urls as url, i (url)}
      {@const size = doc.summary.pages[pages[i]]}
      <img
        class="sheet"
        src={url}
        alt={t.pageOf(pages[i] + 1, doc.summary.page_count)}
        style:aspect-ratio="{size.width_pt} / {size.height_pt}"
        onload={() => markLoaded(url)}
        onerror={() => markLoaded(url)}
      />
    {/each}
  </section>
</div>

<style>
  .print-view {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    flex-direction: column;
    background: var(--bg, #e9e9e9);
  }
  .print-bar {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.4rem 0.75rem;
    background: var(--panel);
    border-bottom: 1px solid var(--panel-border);
  }
  .print-bar h2 {
    margin: 0;
    font-size: 1rem;
  }
  .spacer {
    flex: 1;
  }
  .muted {
    color: var(--muted);
    font-size: 0.85rem;
  }
  .sheets {
    flex: 1;
    overflow: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1rem;
    padding: 1rem;
  }
  .sheet {
    width: min(100%, 800px);
    height: auto;
    background: #fff;
    box-shadow: var(--page-shadow);
  }
  @media print {
    /* Seules les feuilles s'impriment : le reste de l'application est masqué, et la mise en
       page plein écran (hauteur 100 %, débordement caché) qui limiterait l'impression à une
       seule feuille est levée. */
    :global(html),
    :global(body),
    :global(main) {
      height: auto !important;
      overflow: visible !important;
      background: #fff !important;
    }
    :global(main > :not(.print-view)) {
      display: none !important;
    }
    .print-bar {
      display: none;
    }
    .print-view {
      position: static;
      background: #fff;
    }
    .sheets {
      display: block;
      padding: 0;
      overflow: visible;
    }
    .sheet {
      display: block;
      width: auto;
      max-width: 100%;
      max-height: 100vh;
      margin: 0 auto;
      box-shadow: none;
      page-break-after: always;
      break-after: page;
    }
  }
</style>
