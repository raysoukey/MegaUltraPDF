<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { t } from '$lib/strings';

  let {
    path,
    error,
    onsubmit,
    oncancel,
  }: {
    path: string;
    error: string | null;
    onsubmit: (password: string) => void;
    oncancel: () => void;
  } = $props();

  let password = $state('');
  let input: HTMLInputElement | undefined = $state();

  $effect(() => {
    input?.focus();
  });

  function fileName(p: string): string {
    return p.split(/[\\/]/).pop() ?? p;
  }
</script>

<div class="dialog-backdrop" role="presentation">
  <form
    class="dialog"
    aria-labelledby="pwd-title"
    onsubmit={(e) => {
      e.preventDefault();
      onsubmit(password);
    }}
  >
    <h2 id="pwd-title">{t.passwordPrompt}</h2>
    <p class="file">{fileName(path)}</p>
    <p>{t.passwordRequired}</p>
    <input
      bind:this={input}
      bind:value={password}
      type="password"
      placeholder={t.passwordPlaceholder}
      aria-label={t.passwordPrompt}
      autocomplete="off"
    />
    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
    <div class="actions">
      <button type="button" onclick={oncancel}>{t.cancel}</button>
      <button type="submit" class="primary" disabled={password.length === 0}>{t.ok}</button>
    </div>
  </form>
</div>

<style>
  .file {
    color: var(--muted);
    margin: 0 0 0.5rem;
    word-break: break-all;
  }
  input {
    width: 100%;
    box-sizing: border-box;
  }
  .error {
    color: var(--danger-text);
    margin: 0.5rem 0 0;
  }
</style>
