<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import type { OpenDoc } from '$lib/state/documents.svelte';
  import { clearRecent, settings } from '$lib/state/settings.svelte';
  import { shortcutKeyFor } from '$lib/logic/shortcuts';
  import { t } from '$lib/strings';
  import { menuNav } from '$lib/actions/menuNav';

  let {
    doc,
    onopen,
    onopenrecent,
    onexport,
    onprint,
    onproperties,
    onprotect,
    onsign,
    onoptimize,
    oncompare,
    onpreferences,
  }: {
    doc: OpenDoc | null;
    onopen: () => void;
    onopenrecent: (path: string) => void;
    onexport: () => void;
    onprint: () => void;
    onproperties: () => void;
    onprotect: () => void;
    onsign: () => void;
    onoptimize: () => void;
    oncompare: () => void;
    onpreferences: () => void;
  } = $props();

  let open = $state(false);
  let openFrom = $state<'first' | 'last'>('first');
  let trigger: HTMLButtonElement | undefined = $state();

  function close(refocus: boolean) {
    open = false;
    if (refocus) trigger?.focus();
  }
  function pick(action: () => void) {
    // Le focus revient au bouton **avant** d'ouvrir le dialogue : c'est lui que le piège de
    // focus du dialogue retiendra comme élément d'origine (l'élément du menu va disparaître).
    close(true);
    action();
  }
  const entries = $derived<
    { label: string; action: () => void; needsDoc: boolean; key?: string }[]
  >([
    { label: t.exportTitle, action: onexport, needsDoc: true },
    {
      label: t.printTitle,
      action: onprint,
      needsDoc: true,
      key: shortcutKeyFor('print') ?? undefined,
    },
    {
      label: t.properties,
      action: onproperties,
      needsDoc: true,
      key: shortcutKeyFor('properties') ?? undefined,
    },
    { label: t.protectTitle, action: onprotect, needsDoc: true },
    { label: t.signMenu, action: onsign, needsDoc: true },
    { label: t.optimizeTitle, action: onoptimize, needsDoc: true },
    { label: t.compareTitle, action: oncompare, needsDoc: true },
    {
      label: t.preferencesMenu,
      action: onpreferences,
      needsDoc: false,
      key: shortcutKeyFor('preferences') ?? undefined,
    },
  ]);
</script>

<div class="menu">
  <button
    type="button"
    aria-haspopup="menu"
    aria-expanded={open}
    title={t.fileMenu}
    bind:this={trigger}
    onkeydown={(e) => {
      // ↓ ouvre sur le premier élément, ↑ sur le dernier (motif ARIA « bouton de menu »).
      if ((e.key === 'ArrowDown' || e.key === 'ArrowUp') && !open) {
        e.preventDefault();
        openFrom = e.key === 'ArrowUp' ? 'last' : 'first';
        open = true;
      }
    }}
    onclick={() => {
      openFrom = 'first';
      open = !open;
    }}>{t.fileMenu} <span aria-hidden="true">▾</span></button
  >
  {#if open}
    <ul
      class="dropdown"
      role="menu"
      aria-label={t.fileMenu}
      use:menuNav={{ onclose: close, start: openFrom }}
    >
      <li role="none">
        <button type="button" role="menuitem" onclick={() => pick(onopen)}
          >{t.openFile} <span class="key">{shortcutKeyFor('open')}</span></button
        >
      </li>
      {#if settings.recent.length > 0}
        <li class="separator" role="separator"></li>
        <li role="none">
          <ul class="group" role="group" aria-label={t.recentFiles}>
            <li role="none"><span class="muted" aria-hidden="true">{t.recentFiles}</span></li>
            {#each settings.recent.slice(0, 8) as path (path)}
              <li role="none">
                <button
                  type="button"
                  role="menuitem"
                  title={path}
                  onclick={() => pick(() => onopenrecent(path))}>{path.split(/[\\/]/).pop()}</button
                >
              </li>
            {/each}
          </ul>
        </li>
        <li role="none">
          <button type="button" role="menuitem" onclick={() => pick(clearRecent)}
            >{t.clearRecent}</button
          >
        </li>
      {/if}
      <li class="separator" role="separator"></li>
      {#each entries as e (e.label)}
        <li role="none">
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
  .group {
    list-style: none;
    margin: 0;
    padding: 0;
  }
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
    display: block;
    color: var(--muted);
    padding: 0.2rem 0.55rem;
    font-size: 0.85rem;
  }
  .key {
    color: var(--muted);
    font-size: 0.8rem;
  }
</style>
