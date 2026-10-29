<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { nextZoom } from '$lib/logic/zoom';
  import {
    goToPage,
    redo,
    rotateDoc,
    save,
    saveAs,
    setScale,
    undo,
    type OpenDoc,
  } from '$lib/state/documents.svelte';
  import { saveSettings, settings, type Theme } from '$lib/state/settings.svelte';
  import { shortcutKeyFor } from '$lib/logic/shortcuts';
  import { t } from '$lib/strings';
  import FileMenu from './FileMenu.svelte';
  import Icon from './Icon.svelte';

  let {
    doc,
    onopen,
    onopenrecent,
    onfitwidth,
    onfitpage,
    ontogglesearch,
    onproperties,
    onhelp,
    onpreferences,
    onmerge,
    onsplit,
    onexport,
    onprint,
    onprotect,
    onsign,
    onoptimize,
    oncompare,
  }: {
    doc: OpenDoc | null;
    onopen: () => void;
    onopenrecent: (path: string) => void;
    onfitwidth: () => void;
    onfitpage: () => void;
    ontogglesearch: () => void;
    onproperties: () => void;
    onhelp: () => void;
    onpreferences: () => void;
    onmerge: () => void;
    onsplit: () => void;
    onexport: () => void;
    onprint: () => void;
    onprotect: () => void;
    onsign: () => void;
    onoptimize: () => void;
    oncompare: () => void;
  } = $props();

  // Dérivé modifiable : suit la page courante, mais l'utilisateur peut taper dedans.
  let pageInput = $derived(doc ? String(doc.page + 1) : '');

  function submitPage(e: Event) {
    e.preventDefault();
    if (!doc) return;
    const n = Number.parseInt(pageInput, 10);
    if (Number.isFinite(n)) goToPage(doc, n - 1);
  }

  function zoomPercent(d: OpenDoc): string {
    return t.zoomPercent(Math.round(d.scale * 100));
  }

  function setTheme(theme: Theme) {
    settings.theme = theme;
    saveSettings();
  }

  function toggleViewMode() {
    settings.viewMode = settings.viewMode === 'continuous' ? 'single' : 'continuous';
    saveSettings();
  }
</script>

<header class="toolbar">
  <div class="group">
    <button
      type="button"
      class="icon-button"
      aria-label={t.openFile}
      onclick={onopen}
      title="{t.openFile} ({shortcutKeyFor('open')})"><Icon name="open" /></button
    >
    <FileMenu
      {doc}
      {onopen}
      {onopenrecent}
      {onexport}
      {onprint}
      {onproperties}
      {onprotect}
      {onsign}
      {onoptimize}
      {oncompare}
      {onpreferences}
    />
    <button
      aria-label={t.sidebar}
      type="button"
      class:active={settings.sidebarOpen}
      aria-pressed={settings.sidebarOpen}
      title={t.sidebar}
      onclick={() => {
        settings.sidebarOpen = !settings.sidebarOpen;
        saveSettings();
      }}>☰</button
    >
  </div>

  <div class="group" aria-label={t.pageInputLabel}>
    <button
      aria-label={t.previousPage}
      type="button"
      disabled={!doc || doc.page === 0}
      title={t.previousPage}
      onclick={() => doc && goToPage(doc, doc.page - 1)}>‹</button
    >
    <form onsubmit={submitPage}>
      <input
        type="text"
        inputmode="numeric"
        aria-label={t.pageInputLabel}
        bind:value={pageInput}
        disabled={!doc}
        onfocus={(e) => (e.currentTarget as HTMLInputElement).select()}
      />
    </form>
    <span class="muted">/ {doc ? doc.summary.page_count : '–'}</span>
    <button
      aria-label={t.nextPage}
      type="button"
      disabled={!doc || doc.page >= doc.summary.page_count - 1}
      title={t.nextPage}
      onclick={() => doc && goToPage(doc, doc.page + 1)}>›</button
    >
  </div>

  <div class="group">
    <button
      aria-label={t.zoomOut}
      type="button"
      disabled={!doc}
      title="{t.zoomOut} ({shortcutKeyFor('zoomOut')})"
      onclick={() => doc && setScale(doc, nextZoom(doc.scale, -1), 'custom')}>−</button
    >
    <span class="zoom">{doc ? zoomPercent(doc) : '–'}</span>
    <button
      aria-label={t.zoomIn}
      type="button"
      disabled={!doc}
      title="{t.zoomIn} ({shortcutKeyFor('zoomIn')})"
      onclick={() => doc && setScale(doc, nextZoom(doc.scale, 1), 'custom')}>+</button
    >
    <button
      aria-label={t.fitWidth}
      type="button"
      disabled={!doc}
      class:active={doc?.zoomMode === 'fitWidth'}
      title="{t.fitWidth} ({shortcutKeyFor('fitWidth')})"
      onclick={onfitwidth}>↔</button
    >
    <button
      aria-label={t.fitPage}
      type="button"
      disabled={!doc}
      class:active={doc?.zoomMode === 'fitPage'}
      title="{t.fitPage} ({shortcutKeyFor('fitPage')})"
      onclick={onfitpage}>▢</button
    >
    <button
      aria-label={t.rotate}
      type="button"
      disabled={!doc}
      class="icon-button"
      title="{t.rotate} (R)"
      onclick={() => doc && rotateDoc(doc)}><Icon name="rotate" /></button
    >
    <button
      aria-label={settings.viewMode === 'continuous' ? t.viewSingle : t.viewContinuous}
      type="button"
      disabled={!doc}
      title={settings.viewMode === 'continuous' ? t.viewSingle : t.viewContinuous}
      onclick={toggleViewMode}>{settings.viewMode === 'continuous' ? '▯' : '≡'}</button
    >
  </div>

  <div class="group" aria-label={t.editGroup}>
    <button
      aria-label={doc?.edit.undo_label ? t.undoLabel(doc.edit.undo_label) : t.undo}
      type="button"
      disabled={!doc || !doc.edit.can_undo}
      title="{doc?.edit.undo_label ? t.undoLabel(doc.edit.undo_label) : t.undo} ({shortcutKeyFor(
        'undo',
      )})"
      onclick={() => doc && undo(doc)}>↶</button
    >
    <button
      aria-label={doc?.edit.redo_label ? t.redoLabel(doc.edit.redo_label) : t.redo}
      type="button"
      disabled={!doc || !doc.edit.can_redo}
      title="{doc?.edit.redo_label ? t.redoLabel(doc.edit.redo_label) : t.redo} ({shortcutKeyFor(
        'redo',
      )})"
      onclick={() => doc && redo(doc)}>↷</button
    >
    <button
      aria-label={t.save}
      type="button"
      disabled={!doc}
      class="icon-button"
      class:attention={doc?.edit.modified}
      title="{t.save} ({shortcutKeyFor('save')})"
      onclick={() => doc && save(doc)}><Icon name="save" /></button
    >
    <button
      type="button"
      disabled={!doc}
      title="{t.saveAs} ({shortcutKeyFor('saveAs')})"
      onclick={() => doc && saveAs(doc)}>{t.saveAs}</button
    >
    <button
      type="button"
      class="icon-button"
      aria-label="{t.merge}…"
      title="{t.mergeTitle} ({shortcutKeyFor('merge')})"
      onclick={onmerge}><Icon name="merge" /></button
    >
    <button
      type="button"
      class="icon-button"
      aria-label="{t.split}…"
      disabled={!doc}
      title="{t.splitTitle} ({shortcutKeyFor('split')})"
      onclick={onsplit}><Icon name="split" /></button
    >
  </div>

  <div class="group right">
    <button
      aria-label={t.search}
      type="button"
      disabled={!doc}
      class="icon-button"
      title="{t.search} ({shortcutKeyFor('find')})"
      onclick={ontogglesearch}><Icon name="search" /></button
    >
    <button
      aria-label={t.properties}
      type="button"
      disabled={!doc}
      title="{t.properties} ({shortcutKeyFor('properties')})"
      onclick={onproperties}>ⓘ</button
    >
    <select
      aria-label={t.theme}
      value={settings.theme}
      onchange={(e) => setTheme((e.currentTarget as HTMLSelectElement).value as Theme)}
    >
      <option value="system">{t.themeSystem}</option>
      <option value="light">{t.themeLight}</option>
      <option value="dark">{t.themeDark}</option>
    </select>
    <button aria-label={t.helpTitle} type="button" title="{t.helpTitle} (F1)" onclick={onhelp}
      >?</button
    >
  </div>
</header>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0.35rem 0.75rem;
    background: var(--panel);
    border-bottom: 1px solid var(--panel-border);
    flex-wrap: wrap;
  }
  .group {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }
  .right {
    margin-left: auto;
  }
  /* Boutons à icône : moins de marge intérieure, pour garder la hauteur des boutons texte. */
  .icon-button {
    padding: 0.1rem 0.3rem;
  }
  .attention {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .muted {
    color: var(--muted);
  }
  .zoom {
    min-width: 3.5rem;
    text-align: center;
  }
  input[type='text'] {
    width: 3.5rem;
    text-align: center;
  }
</style>
