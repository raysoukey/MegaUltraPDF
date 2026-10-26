<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    closeDocument,
    isAppError,
    openDocument,
    pickPdfFile,
    startupFile,
    type DocumentInfo,
  } from '$lib/api';
  import PageImage from '$lib/components/PageImage.svelte';
  import { t } from '$lib/strings';

  // État de la vue (runes Svelte 5) : le document courant et l'erreur à afficher.
  let doc = $state<DocumentInfo | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  const SCALE = 1.5;

  async function openPath(path: string) {
    error = null;
    busy = true;
    try {
      const previous = doc;
      doc = await openDocument(path);
      // Un seul document à la fois pour l'instant (les onglets arrivent au Lot 1) :
      // on libère l'ancien côté Rust. Un échec de fermeture n'empêche pas l'ouverture.
      if (previous) await closeDocument(previous.id).catch(() => undefined);
    } catch (e) {
      // Toute décision vient du backend ; ici on ne fait qu'afficher.
      if (isAppError(e)) {
        error = e.kind === 'password_required' ? t.passwordRequired : t.openError(e.message);
      } else {
        error = t.openError(String(e));
      }
    } finally {
      busy = false;
    }
  }

  async function onOpen() {
    const path = await pickPdfFile();
    if (path) await openPath(path);
  }

  // Au démarrage : ouvre le fichier passé en argument de ligne de commande, s'il y en a un.
  onMount(async () => {
    const path = await startupFile();
    if (path) await openPath(path);
  });
</script>

<main>
  <header>
    <h1>{t.appName}</h1>
    <button type="button" onclick={onOpen} disabled={busy}>{t.openFile}</button>
    {#if doc}
      <span class="title">{doc.title ?? t.untitled}</span>
      <span class="meta">{t.pages(doc.page_count)}</span>
    {/if}
  </header>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}

  <section class="viewer">
    {#if doc && doc.page_count > 0}
      <PageImage
        docId={doc.id}
        index={0}
        scale={SCALE}
        size={doc.pages[0]}
        total={doc.page_count}
      />
    {:else if !doc}
      <p class="empty">{t.noDocument}</p>
    {/if}
  </section>
</main>

<style>
  :global(body) {
    margin: 0;
    font-family: system-ui, sans-serif;
    background: #e9e9ec;
    color: #1d1d1f;
  }
  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  header {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0.5rem 1rem;
    background: #f7f7f8;
    border-bottom: 1px solid #d0d0d5;
  }
  h1 {
    margin: 0;
    font-size: 1rem;
    font-weight: 600;
  }
  .title {
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    color: #6e6e73;
  }
  .error {
    margin: 0;
    padding: 0.5rem 1rem;
    background: #fde8e8;
    color: #8a1c1c;
    border-bottom: 1px solid #f3c2c2;
  }
  .viewer {
    flex: 1;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding: 1.5rem;
    overflow: auto;
  }
  .empty {
    align-self: center;
    color: #6e6e73;
  }
</style>
