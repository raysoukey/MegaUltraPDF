<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { pickPdfFiles, startupFile } from '$lib/api';
  import AnnotationToolbar from '$lib/components/AnnotationToolbar.svelte';
  import CloseConfirmDialog from '$lib/components/CloseConfirmDialog.svelte';
  import MergeDialog from '$lib/components/MergeDialog.svelte';
  import PasswordDialog from '$lib/components/PasswordDialog.svelte';
  import PropertiesDialog from '$lib/components/PropertiesDialog.svelte';
  import SearchBar from '$lib/components/SearchBar.svelte';
  import ShortcutsHelp from '$lib/components/ShortcutsHelp.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import SplitDialog from '$lib/components/SplitDialog.svelte';
  import Tabs from '$lib/components/Tabs.svelte';
  import Toolbar from '$lib/components/Toolbar.svelte';
  import Viewer from '$lib/components/Viewer.svelte';
  import { selectedText } from '$lib/logic/selection';
  import { actionFor } from '$lib/logic/shortcuts';
  import { nextZoom } from '$lib/logic/zoom';
  import {
    activeDoc,
    cancelPassword,
    deleteSelectedAnnotation,
    displayName,
    docById,
    docsState,
    goToPage,
    openPath,
    openPaths,
    redo,
    rememberAllPositions,
    requestClose,
    requestCloseAll,
    resolveClosePrompt,
    rotateDoc,
    save,
    saveAs,
    setScale,
    selectAnnotation,
    setSelection,
    showNotice,
    stepSearch,
    undo,
  } from '$lib/state/documents.svelte';
  import { flushSettings, loadSettings, settings } from '$lib/state/settings.svelte';
  import { setTool, toolState } from '$lib/state/tools.svelte';
  import { t } from '$lib/strings';
  import { applyTheme } from '$lib/theme';

  const doc = $derived(activeDoc());
  let viewer: Viewer | undefined = $state();
  let searchOpen = $state(false);
  let propertiesOpen = $state(false);
  let helpOpen = $state(false);
  let mergeOpen = $state(false);
  let splitOpen = $state(false);

  $effect(() => {
    applyTheme(settings.theme);
  });

  onMount(() => {
    let unlisten: (() => void) | undefined;
    let unlistenClose: (() => void) | undefined;
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
      try {
        // À la fermeture de la fenêtre : mémoriser les positions de lecture et écrire les
        // préférences tout de suite (la sauvegarde différée n'aurait pas le temps).
        const { getCurrentWindow } = await import('@tauri-apps/api/window');
        unlistenClose = await getCurrentWindow().onCloseRequested(async (event) => {
          // Des modifications en attente : on retient la fermeture, on confirme document
          // par document, puis on redemande la fermeture (plus rien à confirmer).
          if (!requestCloseAll(() => void getCurrentWindow().close())) {
            event.preventDefault();
            return;
          }
          rememberAllPositions();
          await flushSettings();
        });
      } catch {
        // hors Tauri
      }
    })();
    return () => {
      unlisten?.();
      unlistenClose?.();
    };
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
      showNotice(t.copied);
    } catch {
      // presse-papiers indisponible : rien à faire
    }
  }

  function onKeydown(e: KeyboardEvent) {
    const target = e.target as HTMLElement | null;
    const typing =
      target &&
      (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.tagName === 'SELECT');
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'c' && !typing) {
      copySelection();
      return;
    }
    if ((e.key === 'Delete' || e.key === 'Backspace') && !typing && doc?.selectedAnnotation) {
      e.preventDefault();
      deleteSelectedAnnotation(doc);
      return;
    }
    const action = actionFor({
      key: e.key,
      ctrl: e.ctrlKey || e.metaKey,
      shift: e.shiftKey,
      alt: e.altKey,
    });
    if (!action) return;
    // Dans un champ de saisie, seuls les raccourcis avec Ctrl, les touches F et Échap passent ;
    // annuler/rétablir y restent ceux du champ.
    if (typing && !e.ctrlKey && !e.key.startsWith('F') && e.key !== 'Escape') return;
    if (typing && (action === 'undo' || action === 'redo')) return;
    // Un dialogue modal garde le clavier pour lui (sauf Échap).
    if ((docsState.closePrompt || mergeOpen || splitOpen) && action !== 'escape') return;
    e.preventDefault();
    switch (action) {
      case 'open':
        onOpen();
        break;
      case 'close':
        if (doc) requestClose(doc.summary.id);
        break;
      case 'undo':
        if (doc) undo(doc);
        break;
      case 'redo':
        if (doc) redo(doc);
        break;
      case 'save':
        if (doc) save(doc);
        break;
      case 'saveAs':
        if (doc) saveAs(doc);
        break;
      case 'merge':
        mergeOpen = true;
        break;
      case 'split':
        if (doc) splitOpen = true;
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
        if (docsState.closePrompt) resolveClosePrompt('cancel');
        else if (mergeOpen) mergeOpen = false;
        else if (splitOpen) splitOpen = false;
        else if (helpOpen) helpOpen = false;
        else if (propertiesOpen) propertiesOpen = false;
        else if (docsState.passwordPrompt) cancelPassword();
        else if (searchOpen) searchOpen = false;
        else if (toolState.tool !== 'select') setTool('select');
        else if (doc?.selectedAnnotation) selectAnnotation(doc, null);
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
    onmerge={() => (mergeOpen = true)}
    onsplit={() => doc && (splitOpen = true)}
  />
  <Tabs />
  {#if doc}
    <AnnotationToolbar {doc} />
  {/if}
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
  {#if mergeOpen}
    <MergeDialog onclose={() => (mergeOpen = false)} />
  {/if}
  {#if doc && splitOpen}
    <SplitDialog {doc} onclose={() => (splitOpen = false)} />
  {/if}
  {#if docsState.closePrompt}
    {@const closing = docById(docsState.closePrompt.id)}
    <CloseConfirmDialog
      name={closing ? displayName(closing) : t.untitled}
      onchoice={(choice) => resolveClosePrompt(choice)}
    />
  {/if}
  {#if docsState.notice}
    <div class="toast" role="status">{docsState.notice}</div>
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
