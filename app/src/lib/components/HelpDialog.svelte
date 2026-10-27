<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  // Aide intégrée (F1) : prise en main, table des raccourcis générée depuis `shortcuts.ts`,
  // et « À propos » (version, licence, PDFium).
  import { onMount } from 'svelte';
  import { trapFocus } from '$lib/actions/trapFocus';
  import licenses from '$lib/generated/licenses.json';
  import { shortcutHelp } from '$lib/logic/shortcuts';
  import { t } from '$lib/strings';

  let { onclose }: { onclose: () => void } = $props();

  type Tab = 'start' | 'shortcuts' | 'about';
  const TABS: Tab[] = ['start', 'shortcuts', 'about'];
  let tab = $state<Tab>('start');
  let version = $state('');
  // Inventaire généré par `cargo xtask licenses` (commité, regénéré à chaque version).
  const crateCount = licenses.filter((l) => l.source === 'crate').length;

  function tabLabel(k: Tab): string {
    if (k === 'start') return t.helpTabStart;
    if (k === 'shortcuts') return t.helpTabShortcuts;
    return t.helpTabAbout;
  }

  onMount(async () => {
    try {
      const { getVersion } = await import('@tauri-apps/api/app');
      version = await getVersion();
    } catch {
      version = ''; // hors Tauri : pas de version à afficher
    }
  });

  /** ← → passent d'un onglet à l'autre (motif ARIA « onglets »). */
  function onTabKey(e: KeyboardEvent) {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
    e.preventDefault();
    const i = TABS.indexOf(tab);
    tab = TABS[(i + (e.key === 'ArrowRight' ? 1 : TABS.length - 1)) % TABS.length];
    document.getElementById(`help-tab-${tab}`)?.focus();
  }
</script>

<div
  class="dialog-backdrop"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && onclose()}
>
  <div
    class="dialog help"
    role="dialog"
    aria-modal="true"
    aria-labelledby="help-title"
    use:trapFocus
  >
    <h2 id="help-title">{t.helpTitle}</h2>
    <div class="tabs" role="tablist" tabindex="-1" onkeydown={onTabKey}>
      {#each TABS as k (k)}
        <button
          type="button"
          role="tab"
          id="help-tab-{k}"
          aria-selected={tab === k}
          aria-controls="help-panel"
          tabindex={tab === k ? 0 : -1}
          onclick={() => (tab = k)}>{tabLabel(k)}</button
        >
      {/each}
    </div>

    <!-- tabindex="0" : le panneau défile au clavier (motif ARIA « onglets »). -->
    <div
      class="panel"
      id="help-panel"
      role="tabpanel"
      tabindex="0"
      aria-labelledby="help-tab-{tab}"
    >
      {#if tab === 'start'}
        {#each t.helpSections as section (section.title)}
          <h3>{section.title}</h3>
          <p>{section.body}</p>
        {/each}
      {:else if tab === 'shortcuts'}
        <table>
          <tbody>
            {#each shortcutHelp() as row (row.action)}
              <tr>
                <td class="keys"
                  >{#each row.keys as k, i (k)}{#if i > 0},
                    {/if}<kbd>{k}</kbd>{/each}</td
                >
                <td>{row.label}</td>
              </tr>
            {/each}
            <tr><td class="keys"><kbd>{t.ctrlWheel}</kbd></td><td>{t.zoomIn} / {t.zoomOut}</td></tr>
            <tr><td class="keys"><kbd>{t.keyCtrl}+C</kbd></td><td>{t.copy}</td></tr>
          </tbody>
        </table>
      {:else}
        <p class="app">{t.appName}</p>
        <p class="version">{t.aboutVersion(version)}</p>
        <p>{t.aboutTagline}</p>
        <p>{t.aboutLicense}</p>
        <p>{t.aboutPdfium}</p>
        <p>{t.aboutNotices}</p>
        <details>
          <summary>{t.aboutThirdParty(crateCount, licenses.length - crateCount)}</summary>
          <table class="licenses">
            <thead>
              <tr>
                <th>{t.licenseName}</th>
                <th>{t.licenseVersion}</th>
                <th>{t.licenseLicense}</th>
              </tr>
            </thead>
            <tbody>
              {#each licenses as l (l.source + l.name + l.version)}
                <tr>
                  <td>{l.name}</td>
                  <td>{l.version}</td>
                  <td>{l.license ?? t.licenseSeeFile}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </details>
      {/if}
    </div>

    <div class="actions">
      <button type="button" class="primary" onclick={onclose}>{t.close}</button>
    </div>
  </div>
</div>

<style>
  .help {
    width: min(42rem, 90vw);
  }
  .tabs {
    display: flex;
    gap: 0.25rem;
    border-bottom: 1px solid var(--panel-border);
    margin-bottom: 0.75rem;
  }
  .tabs [role='tab'] {
    border-radius: 6px 6px 0 0;
  }
  .tabs [aria-selected='true'] {
    background: var(--accent);
    color: var(--on-accent);
  }
  .panel {
    max-height: 55vh;
    overflow: auto;
  }
  h3 {
    font-size: 0.95rem;
    margin: 0.75rem 0 0.25rem;
  }
  p {
    margin: 0 0 0.5rem;
    line-height: 1.4;
  }
  .app {
    font-size: 1.2rem;
    font-weight: 600;
  }
  .version {
    color: var(--muted);
  }
  table {
    border-collapse: collapse;
  }
  td {
    padding: 0.2rem 1rem 0.2rem 0;
  }
  .keys {
    white-space: nowrap;
  }
  summary {
    cursor: pointer;
    margin: 0.5rem 0;
  }
  .licenses {
    font-size: 0.85rem;
  }
  .licenses th {
    text-align: left;
    padding-right: 1rem;
  }
  kbd {
    background: var(--bg);
    border: 1px solid var(--panel-border);
    border-radius: 4px;
    padding: 0 0.35rem;
    font-family: inherit;
  }
</style>
