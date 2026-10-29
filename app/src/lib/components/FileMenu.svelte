<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import type { OpenDoc } from '$lib/state/documents.svelte';
  import { clearRecent, settings } from '$lib/state/settings.svelte';
  import { shortcutKeyFor } from '$lib/logic/shortcuts';
  import { t } from '$lib/strings';
  import MenuButton from './MenuButton.svelte';
  import Icon from './Icon.svelte';
  import type { IconName } from '$lib/logic/icons';

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

  const entries = $derived<
    { label: string; action: () => void; needsDoc: boolean; key?: string; icon?: IconName }[]
  >([
    { label: t.exportTitle, action: onexport, needsDoc: true, icon: 'export' },
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
    { label: t.protectTitle, action: onprotect, needsDoc: true, icon: 'protect' },
    { label: t.signMenu, action: onsign, needsDoc: true, icon: 'sign' },
    { label: t.optimizeTitle, action: onoptimize, needsDoc: true, icon: 'compress' },
    { label: t.compareTitle, action: oncompare, needsDoc: true },
    {
      label: t.preferencesMenu,
      action: onpreferences,
      needsDoc: false,
      key: shortcutKeyFor('preferences') ?? undefined,
    },
  ]);
</script>

<MenuButton label={t.fileMenu} title={t.fileMenu} minWidth="260px">
  {#snippet items(pick)}
    <li role="none">
      <button type="button" role="menuitem" onclick={() => pick(onopen)}
        ><span class="item"><Icon name="open" size={18} />{t.openFile}</span>
        <span class="key">{shortcutKeyFor('open')}</span></button
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
                onclick={() => pick(() => onopenrecent(path))}
                ><span class="item"
                  ><span class="slot"></span><span class="name">{path.split(/[\\/]/).pop()}</span
                  ></span
                ></button
              >
            </li>
          {/each}
        </ul>
      </li>
      <li role="none">
        <button type="button" role="menuitem" onclick={() => pick(clearRecent)}
          ><span class="item"><span class="slot"></span>{t.clearRecent}</span></button
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
          ><span class="item"
            >{#if e.icon}<Icon name={e.icon} size={18} />{:else}<span class="slot"
              ></span>{/if}{e.label}</span
          >{#if e.key}<span class="key">{e.key}</span>{/if}</button
        >
      </li>
    {/each}
  {/snippet}
</MenuButton>

<style>
  .group {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li button {
    width: 100%;
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Icône et libellé ; les entrées sans icône gardent la place pour rester alignées. */
  .item {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .slot {
    width: 18px;
    flex: none;
  }
  .item {
    min-width: 0;
  }
  /* Un nom de fichier trop long se termine par « … » (le chemin complet est dans l'infobulle). */
  .name {
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
