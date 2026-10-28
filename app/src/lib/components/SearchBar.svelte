<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { clearSearch, runSearch, stepSearch, type OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';

  let { doc, onclose }: { doc: OpenDoc; onclose: () => void } = $props();

  // svelte-ignore state_referenced_locally
  let query = $state(doc.search?.query ?? '');
  let input: HTMLInputElement | undefined = $state();
  let timer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    input?.focus();
    input?.select();
    // À la fermeture de la barre, une recherche différée ne doit plus partir.
    return () => {
      if (timer) clearTimeout(timer);
      timer = null;
    };
  });

  function schedule() {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      runSearch(doc, query, doc.view.matchCase);
    }, 250);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      if (doc.search && doc.search.query === query) stepSearch(doc, e.shiftKey ? -1 : 1);
      else runSearch(doc, query, doc.view.matchCase);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      close();
    }
  }

  function close() {
    clearSearch(doc);
    onclose();
  }

  const count = $derived(
    doc.search ? t.searchCount(doc.search.current + 1, doc.search.hits.length) : '',
  );
</script>

<div class="searchbar" role="search">
  <input
    bind:this={input}
    bind:value={query}
    type="text"
    placeholder={t.searchPlaceholder}
    aria-label={t.search}
    oninput={schedule}
    onkeydown={onKey}
  />
  <label
    ><input
      type="checkbox"
      bind:checked={doc.view.matchCase}
      onchange={() => runSearch(doc, query, doc.view.matchCase)}
    />
    {t.searchMatchCase}</label
  >
  <span class="count" aria-live="polite">{count}</span>
  <button
    aria-label={t.searchPrev}
    type="button"
    title="{t.searchPrev} ({t.keyShift}+F3)"
    disabled={!doc.search?.hits.length}
    onclick={() => stepSearch(doc, -1)}>▲</button
  >
  <button
    aria-label={t.searchNext}
    type="button"
    title="{t.searchNext} (F3)"
    disabled={!doc.search?.hits.length}
    onclick={() => stepSearch(doc, 1)}>▼</button
  >
  <button type="button" title="{t.close} ({t.keyNames.Escape})" aria-label={t.close} onclick={close}
    >×</button
  >
</div>

<style>
  .searchbar {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.35rem 0.75rem;
    background: var(--panel);
    border-bottom: 1px solid var(--panel-border);
  }
  input[type='text'] {
    width: 260px;
  }
  label {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    color: var(--muted);
  }
  .count {
    min-width: 6rem;
    color: var(--muted);
  }
</style>
