<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { tabNav } from '$lib/actions/tabNav';
  import type { Annotation } from '$lib/api';
  import type { OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';
  import AnnotationPanel from './AnnotationPanel.svelte';
  import AttachmentsPanel from './AttachmentsPanel.svelte';
  import FormPanel from './FormPanel.svelte';
  import SignaturesPanel from './SignaturesPanel.svelte';
  import Outline from './Outline.svelte';
  import Thumbnails from './Thumbnails.svelte';

  let {
    doc,
    oneditannotation,
  }: { doc: OpenDoc; oneditannotation: (a: Annotation, page: number) => void } = $props();
  const hasForm = $derived(
    (doc.formInfo?.field_count ?? 0) > 0 || doc.formInfo?.form_type === 'xfa',
  );
</script>

<aside class="sidebar" aria-label={t.sidebar}>
  <div class="tabs" role="tablist" use:tabNav>
    <button
      type="button"
      role="tab"
      aria-selected={doc.view.sidebarTab === 'thumbnails'}
      class:active={doc.view.sidebarTab === 'thumbnails'}
      onclick={() => (doc.view.sidebarTab = 'thumbnails')}>{t.thumbnails}</button
    >
    <button
      type="button"
      role="tab"
      aria-selected={doc.view.sidebarTab === 'bookmarks'}
      class:active={doc.view.sidebarTab === 'bookmarks'}
      onclick={() => (doc.view.sidebarTab = 'bookmarks')}>{t.bookmarks}</button
    >
    <button
      type="button"
      role="tab"
      aria-selected={doc.view.sidebarTab === 'annotations'}
      class:active={doc.view.sidebarTab === 'annotations'}
      onclick={() => (doc.view.sidebarTab = 'annotations')}>{t.annotations}</button
    >
    {#if hasForm}
      <button
        type="button"
        role="tab"
        aria-selected={doc.view.sidebarTab === 'form'}
        class:active={doc.view.sidebarTab === 'form'}
        onclick={() => (doc.view.sidebarTab = 'form')}>{t.formTab}</button
      >
    {/if}
    <button
      type="button"
      role="tab"
      aria-selected={doc.view.sidebarTab === 'attachments'}
      class:active={doc.view.sidebarTab === 'attachments'}
      aria-label={t.attachmentsTab}
      title={t.attachmentsTab}
      onclick={() => (doc.view.sidebarTab = 'attachments')}>📎</button
    >
    <button
      type="button"
      role="tab"
      aria-selected={doc.view.sidebarTab === 'signatures'}
      class:active={doc.view.sidebarTab === 'signatures'}
      aria-label={t.signaturesTab}
      title={t.signaturesTab}
      onclick={() => (doc.view.sidebarTab = 'signatures')}>🔏</button
    >
  </div>
  <div class="content">
    {#if doc.view.sidebarTab === 'thumbnails'}
      <Thumbnails {doc} />
    {:else if doc.view.sidebarTab === 'bookmarks'}
      <Outline {doc} />
    {:else if doc.view.sidebarTab === 'annotations'}
      <AnnotationPanel {doc} onedit={oneditannotation} />
    {:else if doc.view.sidebarTab === 'attachments'}
      <AttachmentsPanel {doc} />
    {:else if doc.view.sidebarTab === 'signatures'}
      <SignaturesPanel {doc} />
    {:else}
      <FormPanel {doc} />
    {/if}
  </div>
</aside>

<style>
  .sidebar {
    width: 220px;
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border-right: 1px solid var(--panel-border);
  }
  .tabs {
    display: flex;
    border-bottom: 1px solid var(--panel-border);
  }
  .tabs button {
    flex: 1;
    border-radius: 0;
  }
  .content {
    flex: 1;
    overflow: auto;
  }
</style>
