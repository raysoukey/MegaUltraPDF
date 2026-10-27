<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import type { Annotation } from '$lib/api';
  import { formatPdfDate } from '$lib/logic/labels';
  import {
    deleteAnnotation,
    ensureAnnotations,
    goToPage,
    selectAnnotation,
    type OpenDoc,
  } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';

  let { doc, onedit }: { doc: OpenDoc; onedit: (a: Annotation, page: number) => void } = $props();

  // Toutes les pages, dans l'ordre ; rechargé quand le cache d'annotations est vidé.
  let pages = $state<{ page: number; items: Annotation[] }[]>([]);
  let loading = $state(true);

  $effect(() => {
    const count = doc.summary.page_count;
    // Lu ici pour relancer l'effet après une édition (le cache est remplacé).
    void doc.annotations;
    let cancelled = false;
    loading = true;
    (async () => {
      const out: { page: number; items: Annotation[] }[] = [];
      for (let p = 0; p < count; p++) {
        const items = (await ensureAnnotations(doc, p)).filter(
          (a) => a.kind !== 'link' && a.kind !== 'widget',
        );
        if (items.length > 0) out.push({ page: p, items });
        if (cancelled) return;
      }
      pages = out;
      loading = false;
    })();
    return () => {
      cancelled = true;
    };
  });

  const total = $derived(pages.reduce((n, p) => n + p.items.length, 0));

  function isSelected(page: number, a: Annotation): boolean {
    return doc.selectedAnnotation?.page === page && doc.selectedAnnotation.index === a.index;
  }
  function excerpt(a: Annotation): string {
    const text = (a.contents ?? '').replace(/\s+/g, ' ').trim();
    return text.length > 60 ? text.slice(0, 57) + '…' : text;
  }
  function pick(page: number, a: Annotation) {
    goToPage(doc, page);
    selectAnnotation(doc, { page, index: a.index });
  }
</script>

<div class="panel" aria-label={t.annotations}>
  {#if loading && pages.length === 0}
    <p class="muted">{t.loading}</p>
  {:else if total === 0}
    <p class="muted">{t.noAnnotations}</p>
  {:else}
    {#each pages as group (group.page)}
      <h3>{t.annotationOnPage(group.page + 1)}</h3>
      <ul role="listbox" aria-label={t.annotationOnPage(group.page + 1)}>
        {#each group.items as a (a.index)}
          <li class:selected={isSelected(group.page, a)}>
            <button
              type="button"
              class="item"
              role="option"
              aria-selected={isSelected(group.page, a)}
              onclick={() => pick(group.page, a)}
              ondblclick={() => onedit(a, group.page)}
            >
              <span class="kind">
                <span
                  class="dot"
                  style:background={a.color
                    ? `rgb(${a.color.r}, ${a.color.g}, ${a.color.b})`
                    : 'transparent'}
                ></span>
                {t.annotationKind[a.kind] ?? a.kind}
              </span>
              <span class="meta">{a.author ?? t.noAuthor} · {formatPdfDate(a.created) ?? ''}</span>
              {#if excerpt(a)}
                <span class="excerpt">{excerpt(a)}</span>
              {/if}
            </button>
            <span class="actions">
              <button
                type="button"
                title={t.editAnnotation}
                aria-label={t.editAnnotation}
                disabled={!a.editable}
                onclick={() => onedit(a, group.page)}>✎</button
              >
              <button
                type="button"
                title={t.deleteAnnotation}
                aria-label={t.deleteAnnotation}
                onclick={() => deleteAnnotation(doc, group.page, a.index)}>×</button
              >
            </span>
          </li>
        {/each}
      </ul>
    {/each}
  {/if}
</div>

<style>
  .panel {
    padding: 0.25rem 0 0.75rem;
  }
  .muted {
    color: var(--muted);
    padding: 0.75rem;
    margin: 0;
  }
  h3 {
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
    margin: 0.75rem 0.75rem 0.25rem;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: flex-start;
    border-left: 3px solid transparent;
  }
  li.selected {
    border-left-color: var(--accent);
    background: var(--accent-soft);
  }
  .item {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    text-align: left;
    padding: 0.35rem 0.5rem;
    border-radius: 0;
    background: none;
  }
  .kind {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    font-weight: 600;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    border: 1px solid var(--panel-border);
  }
  .meta {
    font-size: 0.75rem;
    color: var(--muted);
  }
  .excerpt {
    font-size: 0.8rem;
  }
  .actions {
    display: flex;
    flex-direction: column;
  }
  .actions button {
    padding: 0.1rem 0.4rem;
    line-height: 1.2;
  }
</style>
