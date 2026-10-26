<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { t } from '$lib/strings';

  let {
    name,
    onchoice,
  }: {
    name: string;
    onchoice: (choice: 'save' | 'discard' | 'cancel') => void;
  } = $props();

  let saveButton: HTMLButtonElement | undefined = $state();

  $effect(() => {
    saveButton?.focus();
  });
</script>

<div class="dialog-backdrop" role="presentation">
  <div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="close-title">
    <h2 id="close-title">{t.closeModifiedTitle}</h2>
    <p>{t.closeModifiedQuestion(name)}</p>
    <div class="actions">
      <button type="button" onclick={() => onchoice('cancel')}>{t.cancel}</button>
      <button type="button" class="danger" onclick={() => onchoice('discard')}>{t.dontSave}</button>
      <button type="button" class="primary" bind:this={saveButton} onclick={() => onchoice('save')}
        >{t.save}</button
      >
    </div>
  </div>
</div>

<style>
  .danger {
    color: var(--danger-text);
  }
</style>
