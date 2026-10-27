<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { cancelRedact, confirmRedact, type OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';

  let { doc }: { doc: OpenDoc } = $props();
  let confirmButton: HTMLButtonElement | undefined = $state();
  let busy = $state(false);

  $effect(() => {
    confirmButton?.focus();
  });

  async function confirm() {
    busy = true;
    await confirmRedact(doc);
    busy = false;
  }
</script>

<div class="dialog-backdrop" role="presentation">
  <div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="redact-title">
    <h2 id="redact-title">{t.redactTitle}</h2>
    <p>{t.redactConfirm}</p>
    <div class="actions">
      <button type="button" onclick={() => cancelRedact(doc)}>{t.cancel}</button>
      <button
        type="button"
        class="primary"
        bind:this={confirmButton}
        disabled={busy}
        onclick={confirm}>{t.redact}</button
      >
    </div>
  </div>
</div>
