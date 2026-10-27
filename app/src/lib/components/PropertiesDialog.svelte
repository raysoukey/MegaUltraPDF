<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { documentProperties, fileSize, type DocumentProperties } from '$lib/api';
  import { formatPdfDate } from '$lib/logic/labels';
  import type { OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';

  let { doc, onclose }: { doc: OpenDoc; onclose: () => void } = $props();

  let info = $state<DocumentProperties | null>(null);
  let size = $state<number | null>(null);

  $effect(() => {
    documentProperties(doc.summary.id)
      .then((p) => (info = p))
      .catch(() => (info = null));
    if (doc.summary.path)
      fileSize(doc.summary.path)
        .then((s) => (size = s))
        .catch(() => (size = null));
  });

  function pdfDate(raw: string | null): string {
    return formatPdfDate(raw) ?? t.unknown;
  }

  const rows = $derived.by((): [string, string][] => {
    if (!info) return [];
    return [
      [t.propFile, doc.summary.path ?? t.unknown],
      [t.propSize, size === null ? t.unknown : t.bytes(size)],
      [t.propTitle, info.title ?? t.unknown],
      [t.propAuthor, info.author ?? t.unknown],
      [t.propSubject, info.subject ?? t.unknown],
      [t.propKeywords, info.keywords ?? t.unknown],
      [t.propCreator, info.creator ?? t.unknown],
      [t.propProducer, info.producer ?? t.unknown],
      [t.propCreated, pdfDate(info.creation_date)],
      [t.propModified, pdfDate(info.modification_date)],
      [t.propVersion, info.pdf_version],
      [t.propPages, String(info.page_count)],
    ];
  });
</script>

<div
  class="dialog-backdrop"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && onclose()}
>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="props-title">
    <h2 id="props-title">{t.properties}</h2>
    {#if info === null}
      <p>{t.loading}</p>
    {:else}
      <table>
        <tbody>
          {#each rows as [label, value] (label)}
            <tr><th scope="row">{label}</th><td>{value}</td></tr>
          {/each}
        </tbody>
      </table>
    {/if}
    <div class="actions">
      <button type="button" class="primary" onclick={onclose}>{t.close}</button>
    </div>
  </div>
</div>

<style>
  table {
    border-collapse: collapse;
    min-width: 420px;
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
</style>
