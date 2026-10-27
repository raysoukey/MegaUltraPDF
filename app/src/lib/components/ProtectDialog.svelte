<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import {
    ensureSecurity,
    protectDoc,
    unprotectDoc,
    type OpenDoc,
  } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';

  let { doc, onclose }: { doc: OpenDoc; onclose: () => void } = $props();

  let user = $state('');
  let confirmUser = $state('');
  let owner = $state('');
  let print = $state(true);
  let copy = $state(true);
  let modify = $state(true);
  let annotate = $state(true);
  let busy = $state(false);
  let first: HTMLInputElement | undefined = $state();

  $effect(() => {
    void ensureSecurity(doc);
  });
  $effect(() => {
    first?.focus();
  });

  const encrypted = $derived(doc.security?.encrypted ?? false);
  const mismatch = $derived(user !== confirmUser);
  const canProtect = $derived(
    !busy && !mismatch && (user.trim() !== '' || owner.trim() !== '') && user.length <= 127,
  );

  async function protect() {
    busy = true;
    const ok = await protectDoc(doc, {
      user_password: user,
      owner_password: owner,
      permissions: { print, copy, modify, annotate },
    });
    busy = false;
    if (ok) onclose();
  }
  async function unprotect() {
    busy = true;
    const ok = await unprotectDoc(doc);
    busy = false;
    if (ok) onclose();
  }
</script>

<div
  class="dialog-backdrop"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && onclose()}
>
  <form
    class="dialog"
    aria-labelledby="protect-title"
    onsubmit={(e) => {
      e.preventDefault();
      if (!encrypted) protect();
    }}
  >
    <h2 id="protect-title">{t.protectTitle}</h2>
    {#if encrypted}
      <p>{t.protectAlready}</p>
      {#if doc.security}
        <p class="muted">
          {t.securityPermissions(doc.security.permissions)}
        </p>
      {/if}
      <div class="actions">
        <button type="button" onclick={onclose}>{t.close}</button>
        <button type="button" class="primary" disabled={busy} onclick={unprotect}
          >{t.unprotectAction}</button
        >
      </div>
    {:else}
      <p class="muted">{t.protectHint}</p>
      <label class="row">
        {t.protectUserPassword}
        <input type="password" bind:this={first} bind:value={user} autocomplete="new-password" />
      </label>
      <label class="row">
        {t.protectConfirm}
        <input type="password" bind:value={confirmUser} autocomplete="new-password" />
      </label>
      <label class="row">
        {t.protectOwnerPassword}
        <input type="password" bind:value={owner} autocomplete="new-password" />
      </label>
      {#if mismatch}
        <p class="error" role="alert">{t.protectMismatch}</p>
      {/if}
      <fieldset>
        <legend>{t.protectPermissions}</legend>
        <label><input type="checkbox" bind:checked={print} /> {t.permissionPrint}</label>
        <label><input type="checkbox" bind:checked={copy} /> {t.permissionCopy}</label>
        <label><input type="checkbox" bind:checked={modify} /> {t.permissionModify}</label>
        <label><input type="checkbox" bind:checked={annotate} /> {t.permissionAnnotate}</label>
      </fieldset>
      <div class="actions">
        <button type="button" onclick={onclose}>{t.cancel}</button>
        <button type="submit" class="primary" disabled={!canProtect}>{t.protectAction}</button>
      </div>
    {/if}
  </form>
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    margin: 0.4rem 0;
  }
  .row input {
    width: 14rem;
  }
  fieldset {
    border: 1px solid var(--panel-border);
    border-radius: 6px;
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem 1rem;
    margin-top: 0.5rem;
  }
  .muted {
    color: var(--muted);
    font-size: 0.85rem;
    max-width: 28rem;
  }
  .error {
    color: var(--danger-text);
    margin: 0.2rem 0;
  }
</style>
