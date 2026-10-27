<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { trapFocus } from '$lib/actions/trapFocus';
  import { documentProperties, fileSize, type DocumentProperties } from '$lib/api';
  import { formatPdfDate } from '$lib/logic/labels';
  import { ensureSecurity, setMetadata, type OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';

  let { doc, onclose }: { doc: OpenDoc; onclose: () => void } = $props();

  let info = $state<DocumentProperties | null>(null);
  let size = $state<number | null>(null);
  let title = $state('');
  let author = $state('');
  let subject = $state('');
  let keywords = $state('');
  let busy = $state(false);

  $effect(() => {
    // Relu après chaque édition (le résumé change).
    void doc.summary;
    documentProperties(doc.summary.id)
      .then((p) => {
        info = p;
        title = p.title ?? '';
        author = p.author ?? '';
        subject = p.subject ?? '';
        keywords = p.keywords ?? '';
      })
      .catch(() => (info = null));
    if (doc.summary.path)
      fileSize(doc.summary.path)
        .then((s) => (size = s))
        .catch(() => (size = null));
    void ensureSecurity(doc);
  });

  function pdfDate(raw: string | null): string {
    return formatPdfDate(raw) ?? t.unknown;
  }

  const changed = $derived(
    info !== null &&
      (title !== (info.title ?? '') ||
        author !== (info.author ?? '') ||
        subject !== (info.subject ?? '') ||
        keywords !== (info.keywords ?? '')),
  );
  const readOnly = $derived(doc.security?.encrypted ?? false);

  const rows = $derived.by((): [string, string][] => {
    if (!info) return [];
    return [
      [t.propFile, doc.summary.path ?? t.unknown],
      [t.propSize, size === null ? t.unknown : t.bytes(size)],
      [t.propCreator, info.creator ?? t.unknown],
      [t.propProducer, info.producer ?? t.unknown],
      [t.propCreated, pdfDate(info.creation_date)],
      [t.propModified, pdfDate(info.modification_date)],
      [t.propVersion, info.pdf_version],
      [t.propPages, String(info.page_count)],
    ];
  });

  async function save() {
    if (!info || !changed) return;
    busy = true;
    const patch: Record<string, string> = {};
    if (title !== (info.title ?? '')) patch.title = title;
    if (author !== (info.author ?? '')) patch.author = author;
    if (subject !== (info.subject ?? '')) patch.subject = subject;
    if (keywords !== (info.keywords ?? '')) patch.keywords = keywords;
    await setMetadata(doc, patch);
    busy = false;
  }
</script>

<div
  class="dialog-backdrop"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && onclose()}
>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="props-title" use:trapFocus>
    <form
      class="dialog-form"
      onsubmit={(e) => {
        e.preventDefault();
        save();
      }}
    >
      <h2 id="props-title">{t.properties}</h2>
      {#if info === null}
        <p>{t.loading}</p>
      {:else}
        <label class="field">
          {t.propTitle}
          <input type="text" bind:value={title} maxlength="1024" disabled={readOnly} />
        </label>
        <label class="field">
          {t.propAuthor}
          <input type="text" bind:value={author} maxlength="1024" disabled={readOnly} />
        </label>
        <label class="field">
          {t.propSubject}
          <input type="text" bind:value={subject} maxlength="1024" disabled={readOnly} />
        </label>
        <label class="field">
          {t.propKeywords}
          <input type="text" bind:value={keywords} maxlength="1024" disabled={readOnly} />
        </label>
        {#if readOnly}
          <p class="muted">{t.metadataReadOnlyEncrypted}</p>
        {/if}
        <table>
          <tbody>
            {#each rows as [label, value] (label)}
              <tr><th scope="row">{label}</th><td>{value}</td></tr>
            {/each}
            {#if doc.security}
              <tr>
                <th scope="row">{t.securityTitle}</th>
                <td>
                  {doc.security.encrypted ? t.securityProtected : t.securityOpen}
                  {#if doc.security.encrypted}
                    · {t.securityPermissions(doc.security.permissions)}
                  {/if}
                </td>
              </tr>
            {/if}
          </tbody>
        </table>
      {/if}
      <div class="actions">
        <button type="button" onclick={onclose}>{t.close}</button>
        <button type="submit" class="primary" disabled={!changed || busy}>{t.save}</button>
      </div>
    </form>
  </div>
</div>

<style>
  .field {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    margin: 0.35rem 0;
  }
  .field input {
    width: 20rem;
  }
  table {
    border-collapse: collapse;
    min-width: 420px;
    margin-top: 0.5rem;
  }
  th {
    text-align: left;
    color: var(--muted);
    font-weight: 500;
    padding: 0.25rem 1rem 0.25rem 0;
    white-space: nowrap;
    vertical-align: top;
  }
  td {
    padding: 0.25rem 0;
    word-break: break-all;
  }
  .muted {
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>
