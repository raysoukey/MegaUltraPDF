<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { pickPdfFiles, startupFile } from '$lib/api';
  import PasswordDialog from '$lib/components/PasswordDialog.svelte';
  import PropertiesDialog from '$lib/components/PropertiesDialog.svelte';
  import SearchBar from '$lib/components/SearchBar.svelte';
  import ShortcutsHelp from '$lib/components/ShortcutsHelp.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import Tabs from '$lib/components/Tabs.svelte';
  import Toolbar from '$lib/components/Toolbar.svelte';
  import Viewer from '$lib/components/Viewer.svelte';
  import { selectedText } from '$lib/logic/selection';
  import { actionFor } from '$lib/logic/shortcuts';
  import { nextZoom } from '$lib/logic/zoom';
  import {
    activeDoc,
    cancelPassword,
    closeDoc,
    docsState,
    goToPage,
    openPath,
    openPaths,
    rotateDoc,
    setScale,
    setSelection,
    stepSearch,
  } from '$lib/state/documents.svelte';
  import { loadSettings, settings } from '$lib/state/settings.svelte';
  import { t } from '$lib/strings';
  import { applyTheme } from '$lib/theme';

  const doc = $derived(activeDoc());
  let viewer: Viewer | undefined = $state();
  let searchOpen = $state(false);
  let propertiesOpen = $state(false);
  let helpOpen = $state(false);
  let toast = $state<string | null>(null);

  $effect(() => {
    applyTheme(settings.theme);
  });

  onMount(() => {
    let unlisten: (() => void) | undefined;
    (async () => {
      await loadSettings();
      const path = await startupFile().catch(() => null);
      if (path) await openPath(path);
      try {
        const { getCurrentWebview } = await import('@tauri-apps/api/webview');
        unlisten = await getCurrentWebview().onDragDropEvent((event) => {
          if (event.payload.type === 'drop') openPaths(event.payload.paths);
        });
      } catch {
        // hors Tauri : pas de glisser-déposer natif
      }
    })();
    return () => unlisten?.();
  });

  async function onOpen() {
    const paths = await pickPdfFiles();
    await openPaths(paths);
  }

  async function copySelection() {
    if (!doc?.selection) return;
    const boxes = doc.textLayers[doc.selection.page] ?? [];
    const text = selectedText(boxes, doc.selection.from, doc.selection.to);
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      showToast(t.copied);
    } catch {
      // presse-papiers indisponible : rien à faire
    }
  }

  function showToast(message: string) {
    toast = message;
    setTimeout(() => (toast = null), 1500);
  }

  function onKeydown(e: KeyboardEvent) {
    const target = e.target as HTMLElement | null;
    const typing =
      target &&
      (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.tagName === 'SELECT');
    if (e.ctrlKey && e.key.toLowerCase() === 'c' && !typing) {
      copySelection();
      return;
    }
    const action = actionFor({
      key: e.key,
      ctrl: e.ctrlKey || e.metaKey,
      shift: e.shiftKey,
      alt: e.altKey,
    });
    if (!action) return;
    // Dans un champ de saisie, seuls les raccourcis avec Ctrl, les touches F et Échap passent.
    if (typing && !e.ctrlKey && !e.key.startsWith('F') && e.key !== 'Escape') return;
    e.preventDefault();
    switch (action) {
      case 'open':
        onOpen();
        break;
      case 'close':
        if (doc) closeDoc(doc.summary.id);
        break;
      case 'find':
        if (doc) searchOpen = true;
        break;
      case 'findNext':
        if (doc) stepSearch(doc, 1);
        break;
      case 'findPrev':
        if (doc) stepSearch(doc, -1);
        break;
      case 'zoomIn':
        if (doc) setScale(doc, nextZoom(doc.scale, 1), 'custom');
        break;
      case 'zoomOut':
        if (doc) setScale(doc, nextZoom(doc.scale, -1), 'custom');
        break;
      case 'zoomReset':
        if (doc) setScale(doc, 1, 'custom');
        break;
      case 'fitWidth':
        viewer?.fitWidth();
        break;
      case 'fitPage':
        viewer?.fitPage();
        break;
      case 'rotate':
        if (doc) rotateDoc(doc);
        break;
      case 'firstPage':
        if (doc) goToPage(doc, 0);
        break;
      case 'lastPage':
        if (doc) goToPage(doc, doc.summary.page_count - 1);
        break;
      case 'nextPage':
        if (doc) goToPage(doc, doc.page + 1);
        break;
      case 'prevPage':
        if (doc) goToPage(doc, doc.page - 1);
        break;
      case 'help':
        helpOpen = !helpOpen;
        break;
      case 'properties':
        if (doc) propertiesOpen = true;
        break;
      case 'escape':
        if (helpOpen) helpOpen = false;
        else if (propertiesOpen) propertiesOpen = false;
        else if (docsState.passwordPrompt) cancelPassword();
        else if (searchOpen) searchOpen = false;
        else if (doc?.selection) setSelection(doc, null);
        break;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<main>
  <Toolbar
    {doc}
    onopen={onOpen}
    onopenrecent={(p) => openPath(p)}
    onfitwidth={() => viewer?.fitWidth()}
    onfitpage={() => viewer?.fitPage()}
    ontogglesearch={() => (searchOpen = !searchOpen)}
    onproperties={() => (propertiesOpen = true)}
    onhelp={() => (helpOpen = true)}
  />
  <Tabs />
  {#if doc && searchOpen}
    <SearchBar {doc} onclose={() => (searchOpen = false)} />
  {/if}
  {#if docsState.error}
    <p class="error" role="alert">{docsState.error}</p>
  {/if}

  <section class="body">
    {#if doc}
      {#if settings.sidebarOpen}
        <Sidebar {doc} />
      {/if}
      {#key doc.summary.id}
        <Viewer bind:this={viewer} {doc} />
      {/key}
    {:else}
      <div class="empty">
        <p>{t.noDocument}</p>
        <p class="muted">{t.dropHint}</p>
      </div>
    {/if}
  </section>

  {#if docsState.passwordPrompt}
    <PasswordDialog
      path={docsState.passwordPrompt.path}
      error={docsState.passwordPrompt.error}
      onsubmit={(pwd) => docsState.passwordPrompt && openPath(docsState.passwordPrompt.path, pwd)}
      oncancel={cancelPassword}
    />
  {/if}
  {#if doc && propertiesOpen}
    <PropertiesDialog {doc} onclose={() => (propertiesOpen = false)} />
  {/if}
  {#if helpOpen}
    <ShortcutsHelp onclose={() => (helpOpen = false)} />
  {/if}
  {#if toast}
    <div class="toast" role="status">{toast}</div>
  {/if}
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .error {
    margin: 0;
    padding: 0.5rem 1rem;
    background: var(--danger-bg);
    color: var(--danger-text);
    border-bottom: 1px solid var(--danger-border);
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.25rem;
  }
  .muted {
    color: var(--muted);
  }
  .toast {
    position: fixed;
    bottom: 1.5rem;
    left: 50%;
    transform: translateX(-50%);
    background: var(--panel);
    border: 1px solid var(--panel-border);
    border-radius: 8px;
    padding: 0.4rem 0.9rem;
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.25);
  }
</style>
