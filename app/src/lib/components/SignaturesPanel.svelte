<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  // Signatures numériques du fichier, vérifiées par le cœur (Lot 8). Les noms et motifs
  // viennent du PDF : affichés comme du texte, jamais interprétés.
  import { signatureVerdict } from '$lib/logic/signatures';
  import { ensureSignatures, type OpenDoc } from '$lib/state/documents.svelte';
  import { ui } from '$lib/state/settings.svelte';
  import { t } from '$lib/strings';

  let { doc }: { doc: OpenDoc } = $props();

  $effect(() => {
    void ensureSignatures(doc);
  });

  function date(unix: number): string {
    return new Date(unix * 1000).toLocaleString(ui.language);
  }
</script>

<div class="signatures">
  {#if doc.signatures === null}
    <p class="muted">{t.loading}</p>
  {:else if doc.signatures.length === 0}
    <p class="muted">{t.signaturesEmpty}</p>
  {:else}
    <ul>
      {#each doc.signatures as sig, i (i)}
        {@const verdict = signatureVerdict(sig)}
        <li class={verdict.tone}>
          <p class="verdict">{verdict.text}</p>
          <dl>
            {#if sig.signer}<dt>{t.sigSigner}</dt>
              <dd>{sig.signer}</dd>{/if}
            {#if sig.signed_at !== null}<dt>{t.sigDate}</dt>
              <dd>{date(sig.signed_at)}</dd>{/if}
            {#if sig.reason}<dt>{t.sigReason}</dt>
              <dd>{sig.reason}</dd>{/if}
            {#if sig.location}<dt>{t.sigLocation}</dt>
              <dd>{sig.location}</dd>{/if}
          </dl>
        </li>
      {/each}
    </ul>
    <p class="note">{t.signaturesNote}</p>
  {/if}
</div>

<style>
  .signatures {
    padding: 0.5rem;
    font-size: 0.85rem;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  li {
    border: 1px solid var(--panel-border);
    border-left: 4px solid var(--muted);
    border-radius: 6px;
    padding: 0.4rem 0.5rem;
  }
  li.ok {
    border-left-color: #2e9d57;
  }
  li.warn {
    border-left-color: #d08a1a;
  }
  li.bad {
    border-left-color: var(--danger-text);
  }
  .verdict {
    margin: 0 0 0.25rem;
    font-weight: 600;
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.1rem 0.5rem;
    margin: 0;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .muted,
  .note {
    color: var(--muted);
  }
  .note {
    margin-top: 0.75rem;
    line-height: 1.35;
  }
</style>
