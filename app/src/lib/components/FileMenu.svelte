<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import type { OpenDoc } from '$lib/state/documents.svelte';
  import { clearRecent, settings } from '$lib/state/settings.svelte';
  import { t } from '$lib/strings';

  let {
    doc,
    onopen,
    onopenrecent,
    onexport,
    onprint,
    onproperties,
    onprotect,
    onoptimize,
    oncompare,
  }: {
    doc: OpenDoc | null;
    onopen: () => void;
    onopenrecent: (path: string) => void;
    onexport: () => void;
    onprint: () => void;
    onproperties: () => void;
    onprotect: () => void;
    onoptimize: () => void;
    oncompare: () => void;
  } = $props();

  let open = $state(false);

  function pick(action: () => void) {
    open = false;
    action();
  }
  const entries = $derived<
    { label: string; action: () => void; needsDoc: boolean; key?: string }[]
  >([
    { label: t.exportTitle, action: onexport, needsDoc: true },
    { label: t.printTitle, action: onprint, needsDoc: true, key: 'Ctrl+P' },
    { label: t.properties, action: onproperties, needsDoc: true, key: 'Ctrl+I' },
    { label: t.protectTitle, action: onprotect, needsDoc: true },
    { label: t.optimizeTitle, action: onoptimize, needsDoc: true },
    { label: t.compareTitle, action: oncompare, needsDoc: true },
  ]);
</script>

<div class="menu">
  <button
    type="button"
    aria-haspopup="menu"
    aria-expanded={open}
    title={t.fileMenu}
    onclick={() => (open = !open)}>{t.fileMenu} ▾</button
  >
  {#if open}
    <ul class="dropdown" role="menu" onmouseleave={() => (open = false)}>
      <li>
        <button type="button" role="menuitem" onclick={() => pick(onopen)}
          >{t.openFile} <span class="key">Ctrl+O</span></button
        >
      </li>
      {#if settings.recent.length > 0}
        <li class="separator"></li>
        <li class="muted">{t.recentFiles}</li>
        {#each settings.recent.slice(0, 8) as path (path)}
          <li>
            <button
              type="button"
              role="menuitem"
              title={path}
              onclick={() => pick(() => onopenrecent(path))}>{path.split(/[\\/]/).pop()}</button
            >
          </li>
        {/each}
        <li>
          <button type="button" role="menuitem" onclick={() => pick(clearRecent)}
            >{t.clearRecent}</button
          >
        </li>
      {/if}
      <li class="separator"></li>
      {#each entries as e (e.label)}
        <li>
          <button
            type="button"
            role="menuitem"
            disabled={e.needsDoc && !doc}
            onclick={() => pick(e.action)}
            >{e.label}{#if e.key}<span class="key">{e.key}</span>{/if}</button
          >
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .menu {
    position: relative;
  }
  .dropdown {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 20;
    list-style: none;
    margin: 0;
    padding: 0.25rem;
    min-width: 260px;
    background: var(--panel);
    border: 1px solid var(--panel-border);
    border-radius: 8px;
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.25);
  }
  .dropdown li button {
    width: 100%;
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .separator {
    border-top: 1px solid var(--panel-border);
    margin: 0.25rem 0;
  }
  .muted {
    color: var(--muted);
    padding: 0.2rem 0.55rem;
    font-size: 0.85rem;
  }
  .key {
    color: var(--muted);
    font-size: 0.8rem;
  }
</style>
