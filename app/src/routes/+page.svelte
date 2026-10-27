<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { pickPdfFiles, startupFile } from '$lib/api';
  import type { Annotation } from '$lib/api';
  import AnnotationEditor from '$lib/components/AnnotationEditor.svelte';
  import AnnotationToolbar from '$lib/components/AnnotationToolbar.svelte';
  import SignatureDialog from '$lib/components/SignatureDialog.svelte';
  import CloseConfirmDialog from '$lib/components/CloseConfirmDialog.svelte';
  import CompareDialog from '$lib/components/CompareDialog.svelte';
  import CropDialog from '$lib/components/CropDialog.svelte';
  import ExportDialog from '$lib/components/ExportDialog.svelte';
  import OptimizeDialog from '$lib/components/OptimizeDialog.svelte';
  import PrintView from '$lib/components/PrintView.svelte';
  import ProtectDialog from '$lib/components/ProtectDialog.svelte';
  import NumberingDialog from '$lib/components/NumberingDialog.svelte';
  import RedactConfirmDialog from '$lib/components/RedactConfirmDialog.svelte';
  import TextInsertDialog from '$lib/components/TextInsertDialog.svelte';
  import WatermarkDialog from '$lib/components/WatermarkDialog.svelte';
  import MergeDialog from '$lib/components/MergeDialog.svelte';
  import PasswordDialog from '$lib/components/PasswordDialog.svelte';
  import PropertiesDialog from '$lib/components/PropertiesDialog.svelte';
  import SearchBar from '$lib/components/SearchBar.svelte';
  import HelpDialog from '$lib/components/HelpDialog.svelte';
  import PreferencesDialog from '$lib/components/PreferencesDialog.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import SplitDialog from '$lib/components/SplitDialog.svelte';
  import Tabs from '$lib/components/Tabs.svelte';
  import Toolbar from '$lib/components/Toolbar.svelte';
  import Viewer from '$lib/components/Viewer.svelte';
  import { isContentTool } from '$lib/logic/content';
  import { selectedText } from '$lib/logic/selection';
  import { actionFor } from '$lib/logic/shortcuts';
  import { nextZoom } from '$lib/logic/zoom';
  import {
    activeDoc,
    annotateSelection,
    cancelPassword,
    cancelRedact,
    cancelSignature,
    cancelTextInsert,
    deleteSelectedAnnotation,
    deleteSelectedObject,
    displayName,
    ensureAnnotations,
    ensureFormInfo,
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
    selectObject,
    setSelection,
    showNotice,
    stepSearch,
    undo,
  } from '$lib/state/documents.svelte';
  import {
    applyLanguage,
    flushSettings,
    loadSettings,
    settings,
    ui,
  } from '$lib/state/settings.svelte';
  import { currentStyle, setTool, toolState } from '$lib/state/tools.svelte';
  import { t } from '$lib/strings';
  import { applyTheme } from '$lib/theme';

  const doc = $derived(activeDoc());
  let viewer: Viewer | undefined = $state();
  let searchOpen = $state(false);
  let propertiesOpen = $state(false);
  let helpOpen = $state(false);
  let preferencesOpen = $state(false);
  let mergeOpen = $state(false);
  let splitOpen = $state(false);
  let watermarkOpen = $state(false);
  let numberingOpen = $state(false);
  let cropOpen = $state(false);
  let exportOpen = $state(false);
  let printOpen = $state(false);
  let protectOpen = $state(false);
  let optimizeOpen = $state(false);
  let compareOpen = $state(false);
  const contentDialogOpen = $derived(
    watermarkOpen ||
      numberingOpen ||
      cropOpen ||
      exportOpen ||
      printOpen ||
      protectOpen ||
      optimizeOpen ||
      compareOpen ||
      !!doc?.textInsertRequest ||
      !!doc?.redactRequest,
  );
  let editing = $state<{ annotation: Annotation; page: number } | null>(null);

  async function editSelected() {
    if (!doc?.selectedAnnotation) return;
    const { page, index } = doc.selectedAnnotation;
    const a = (await ensureAnnotations(doc, page)).find((x) => x.index === index);
    if (a) editing = { annotation: a, page };
  }

  async function markupSelection(kind: 'highlight' | 'underline' | 'strike_out') {
    if (!doc) return;
    const tool =
      kind === 'highlight' ? 'highlight' : kind === 'underline' ? 'underline' : 'strikeout';
    const ok = await annotateSelection(doc, kind, currentStyle(tool), settings.author);
    if (!ok) showNotice(t.selectTextFirst);
  }

  $effect(() => {
    applyTheme(settings.theme);
  });

  // Après un changement de langue, l'interface est remontée : chaque visionneuse doit revenir
  // à la page où l'on était (et non en haut du document).
  $effect(() => {
    void ui.language;
    for (const d of docsState.docs) d.scrollRequest = d.page;
  });

  // Nature du formulaire du document actif (onglet « Formulaire », bandeau XFA).
  $effect(() => {
    if (doc && !doc.formInfo) void ensureFormInfo(doc);
  });

  onMount(() => {
    let unlisten: (() => void) | undefined;
    let unlistenClose: (() => void) | undefined;
    (async () => {
      await loadSettings();
      applyLanguage();
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
    // Ctrl+P n'imprime jamais la fenêtre telle quelle (impression native du WebView) : seule
    // la vue d'impression le fait, même si un dialogue est ouvert.
    if (action === 'print') e.preventDefault();
    // Un dialogue modal garde le clavier pour lui (sauf Échap ; F1 referme l'aide).
    if (
      (docsState.closePrompt ||
        docsState.passwordPrompt ||
        propertiesOpen ||
        preferencesOpen ||
        (helpOpen && action !== 'help') ||
        mergeOpen ||
        splitOpen ||
        editing ||
        doc?.signatureRequest ||
        contentDialogOpen) &&
      action !== 'escape'
    )
      return;
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
      case 'toolNote':
        if (doc) setTool('note');
        break;
      case 'toolText':
        if (doc) setTool('text');
        break;
      case 'toolInk':
        if (doc) setTool('ink');
        break;
      case 'toolHighlight':
        markupSelection('highlight');
        break;
      case 'toolUnderline':
        markupSelection('underline');
        break;
      case 'toolStrikeout':
        markupSelection('strike_out');
        break;
      case 'deleteAnnotation':
        if (doc?.selectedObject && isContentTool(toolState.tool)) deleteSelectedObject(doc);
        else if (doc) deleteSelectedAnnotation(doc);
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
      case 'preferences':
        preferencesOpen = true;
        break;
      case 'properties':
        if (doc) propertiesOpen = true;
        break;
      case 'print':
        if (doc) printOpen = true;
        break;
      case 'escape':
        if (docsState.closePrompt) resolveClosePrompt('cancel');
        else if (doc?.signatureRequest) cancelSignature(doc);
        else if (doc?.textInsertRequest) cancelTextInsert(doc);
        else if (doc?.redactRequest) cancelRedact(doc);
        else if (watermarkOpen) watermarkOpen = false;
        else if (numberingOpen) numberingOpen = false;
        else if (cropOpen) cropOpen = false;
        else if (printOpen) printOpen = false;
        else if (exportOpen) exportOpen = false;
        else if (protectOpen) protectOpen = false;
        else if (optimizeOpen) optimizeOpen = false;
        else if (compareOpen) compareOpen = false;
        else if (editing) editing = null;
        else if (mergeOpen) mergeOpen = false;
        else if (splitOpen) splitOpen = false;
        else if (helpOpen) helpOpen = false;
        else if (preferencesOpen) preferencesOpen = false;
        else if (propertiesOpen) propertiesOpen = false;
        else if (docsState.passwordPrompt) cancelPassword();
        else if (searchOpen) searchOpen = false;
        else if (doc?.selectedObject) selectObject(doc, null);
        else if (toolState.tool !== 'select') setTool('select');
        else if (doc?.selectedAnnotation) selectAnnotation(doc, null);
        else if (doc?.selection) setSelection(doc, null);
        break;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#key ui.language}
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
      onpreferences={() => (preferencesOpen = true)}
      onmerge={() => (mergeOpen = true)}
      onsplit={() => doc && (splitOpen = true)}
      onexport={() => doc && (exportOpen = true)}
      onprint={() => doc && (printOpen = true)}
      onprotect={() => doc && (protectOpen = true)}
      onoptimize={() => doc && (optimizeOpen = true)}
      oncompare={() => doc && (compareOpen = true)}
    />
    <Tabs />
    {#if doc}
      <AnnotationToolbar
        {doc}
        onedit={editSelected}
        onwatermark={() => (watermarkOpen = true)}
        onnumbering={() => (numberingOpen = true)}
        oncrop={() => (cropOpen = true)}
      />
    {/if}
    {#if doc && searchOpen}
      <SearchBar {doc} onclose={() => (searchOpen = false)} />
    {/if}
    {#if docsState.error}
      <p class="error" role="alert">{docsState.error}</p>
    {/if}
    {#if doc?.formInfo?.form_type === 'xfa'}
      <p class="error" role="status">{t.formXfa}</p>
    {/if}

    <section class="body">
      {#if doc}
        {#if settings.sidebarOpen}
          <Sidebar {doc} oneditannotation={(a, page) => (editing = { annotation: a, page })} />
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
      <HelpDialog onclose={() => (helpOpen = false)} />
    {/if}
    {#if preferencesOpen}
      <PreferencesDialog onclose={() => (preferencesOpen = false)} />
    {/if}
    {#if mergeOpen}
      <MergeDialog onclose={() => (mergeOpen = false)} />
    {/if}
    {#if doc && splitOpen}
      <SplitDialog {doc} onclose={() => (splitOpen = false)} />
    {/if}
    {#if doc && doc.signatureRequest}
      <SignatureDialog {doc} page={doc.signatureRequest.page} rect={doc.signatureRequest.rect} />
    {/if}
    {#if doc && doc.textInsertRequest}
      <TextInsertDialog {doc} page={doc.textInsertRequest.page} at={doc.textInsertRequest.at} />
    {/if}
    {#if doc && doc.redactRequest}
      <RedactConfirmDialog {doc} />
    {/if}
    {#if doc && watermarkOpen}
      <WatermarkDialog {doc} onclose={() => (watermarkOpen = false)} />
    {/if}
    {#if doc && numberingOpen}
      <NumberingDialog {doc} onclose={() => (numberingOpen = false)} />
    {/if}
    {#if doc && cropOpen}
      <CropDialog {doc} onclose={() => (cropOpen = false)} />
    {/if}
    {#if doc && exportOpen}
      <ExportDialog {doc} onclose={() => (exportOpen = false)} />
    {/if}
    {#if doc && protectOpen}
      <ProtectDialog {doc} onclose={() => (protectOpen = false)} />
    {/if}
    {#if doc && optimizeOpen}
      <OptimizeDialog {doc} onclose={() => (optimizeOpen = false)} />
    {/if}
    {#if doc && compareOpen}
      <CompareDialog {doc} onclose={() => (compareOpen = false)} />
    {/if}
    {#if doc && printOpen}
      <PrintView {doc} onclose={() => (printOpen = false)} />
    {/if}
    {#if doc && editing}
      {#key editing.annotation.index}
        <AnnotationEditor
          {doc}
          page={editing.page}
          annotation={editing.annotation}
          onclose={() => (editing = null)}
        />
      {/key}
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
{/key}

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
