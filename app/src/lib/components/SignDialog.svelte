<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  // Signature numérique (Lot 8) : certificat PKCS#12, mot de passe, informations, puis
  // « Signer et enregistrer sous… ». Le mot de passe ne quitte ce dialogue que pour la
  // commande de signature ; il n'est jamais gardé.
  import { trapFocus } from '$lib/actions/trapFocus';
  import { pickCertificateFile, pickImageFile } from '$lib/api';
  import { stampRect, type StampCorner } from '$lib/logic/stamp';
  import { signAndSave, type OpenDoc } from '$lib/state/documents.svelte';
  import { settings } from '$lib/state/settings.svelte';
  import { t } from '$lib/strings';

  // `busy` est lié par la page : Échap ne ferme pas le dialogue pendant la signature.
  let {
    doc,
    onclose,
    busy = $bindable(false),
  }: { doc: OpenDoc; onclose: () => void; busy?: boolean } = $props();

  let certificate = $state<string | null>(null);
  let password = $state('');
  let reason = $state('');
  let location = $state('');
  let contact = $state(settings.author);
  let visible = $state(false);
  let corner = $state<StampCorner>('bottom_right');
  let image = $state<string | null>(null);
  let error = $state('');
  const corners: StampCorner[] = ['bottom_right', 'bottom_left', 'top_right', 'top_left'];

  const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p;
  const orNull = (s: string) => (s.trim() ? s.trim() : null);

  async function choose() {
    const picked = await pickCertificateFile();
    if (picked) certificate = picked;
  }

  async function chooseImage() {
    const picked = await pickImageFile();
    if (picked) image = picked;
  }

  /** Tampon sur la page courante, au coin choisi (taille affichée de la page). */
  function appearance() {
    const size = doc.summary.pages[doc.page];
    if (!visible || !size) return null;
    return { page: doc.page, rect: stampRect(size, corner), image_path: image };
  }

  async function submit() {
    if (!certificate) return;
    busy = true;
    error = '';
    const result = await signAndSave(doc, certificate, password, {
      reason: orNull(reason),
      location: orNull(location),
      contact: orNull(contact),
      appearance: appearance(),
    });
    busy = false;
    if (result === null) onclose();
    else error = result;
  }
</script>

<div
  class="dialog-backdrop"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && !busy && onclose()}
>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="sign-title" use:trapFocus>
    <form
      class="dialog-form"
      onsubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <h2 id="sign-title">{t.signTitle}</h2>
      <p class="intro">{t.signIntro}</p>
      <div class="grid">
        <span>{t.signCertificate}</span>
        <span class="file">
          <button type="button" onclick={choose} disabled={busy}>{t.signChoose}</button>
          <span class="name">{certificate ? fileName(certificate) : t.signNoCertificate}</span>
        </span>
        <label for="sign-password">{t.signPassword}</label>
        <input id="sign-password" type="password" autocomplete="off" bind:value={password} />
        <label for="sign-reason">{t.signReason}</label>
        <input id="sign-reason" type="text" maxlength="200" bind:value={reason} />
        <label for="sign-location">{t.signLocation}</label>
        <input id="sign-location" type="text" maxlength="200" bind:value={location} />
        <label for="sign-contact">{t.signContact}</label>
        <input id="sign-contact" type="text" maxlength="200" bind:value={contact} />
        <span></span>
        <label class="check"><input type="checkbox" bind:checked={visible} /> {t.signVisible}</label
        >
        {#if visible}
          <label for="sign-corner">{t.signCorner}</label>
          <select id="sign-corner" bind:value={corner}>
            {#each corners as c (c)}
              <option value={c}>{t.signCorners[c]}</option>
            {/each}
          </select>
          <span>{t.signImage}</span>
          <span class="file">
            <button type="button" onclick={chooseImage} disabled={busy}>{t.signChoose}</button>
            <span class="name">{image ? fileName(image) : t.signNoImage}</span>
          </span>
        {/if}
      </div>
      {#if error}
        <p class="error" role="alert">{error}</p>
      {/if}
      <div class="actions">
        <button type="button" onclick={onclose} disabled={busy}>{t.cancel}</button>
        <button type="submit" class="primary" disabled={!certificate || busy}>{t.signSubmit}</button
        >
      </div>
    </form>
  </div>
</div>

<style>
  .intro {
    max-width: 30rem;
    line-height: 1.4;
  }
  .grid {
    display: grid;
    grid-template-columns: max-content minmax(14rem, 1fr);
    gap: 0.5rem 0.75rem;
    align-items: center;
  }
  .file {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    min-width: 0;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
  }
  .check {
    display: flex;
    gap: 0.4rem;
    align-items: center;
  }
  .error {
    color: var(--danger-text);
    background: var(--danger-bg);
    border-radius: 6px;
    padding: 0.4rem 0.6rem;
  }
</style>
