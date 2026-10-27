<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  // Reconnaissance du texte (Lot 8) : une commande `ocr_page` par page, enchaînées ici pour
  // afficher la progression et permettre d'arrêter entre deux pages.
  import { onMount } from 'svelte';
  import { trapFocus } from '$lib/actions/trapFocus';
  import { ocrStatus, type OcrStatus } from '$lib/api';
  import { ocrOutcome, ocrPages, type OcrScope } from '$lib/logic/ocr';
  import { ocrPage, pagesWithText, showNotice, type OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';

  // `running` est lié par la page : Échap ne ferme pas le dialogue pendant la reconnaissance
  // (la boucle continuerait sans affichage ni bouton Arrêter).
  let {
    doc,
    onclose,
    running = $bindable(false),
  }: { doc: OpenDoc; onclose: () => void; running?: boolean } = $props();

  let status = $state<OcrStatus | null>(null);
  let withText = $state<boolean[]>([]);
  let chosen = $state<string[]>([]);
  let scope = $state<OcrScope>('untexted');
  let stopRequested = false;
  let progress = $state('');
  let message = $state('');

  onMount(async () => {
    const [s, pages] = await Promise.all([
      ocrStatus().catch(() => ({ available: false, path: null, languages: [] })),
      pagesWithText(doc),
    ]);
    status = s;
    // Lecture impossible : on le dit, plutôt que « elles contiennent déjà du texte ».
    if (pages === null) message = t.ocrPagesUnknown;
    else withText = pages;
    // Par défaut : français et anglais s'ils sont installés, sinon la première langue.
    const preferred = s.languages.filter((l) => l === 'fra' || l === 'eng');
    chosen = preferred.length > 0 ? preferred : s.languages.slice(0, 1);
  });

  const targets = $derived(ocrPages(scope, doc.page, withText));

  function toggle(lang: string, on: boolean) {
    chosen = on ? [...chosen, lang] : chosen.filter((l) => l !== lang);
  }

  async function run() {
    message = '';
    if (chosen.length === 0) {
      message = t.ocrNeedLanguage;
      return;
    }
    if (targets.length === 0) {
      message = t.ocrNothing;
      return;
    }
    running = true;
    stopRequested = false;
    const pages = [...targets];
    const processed: number[] = [];
    for (const page of pages) {
      if (stopRequested) break;
      progress = t.ocrProgress(processed.length + 1, pages.length);
      if (!(await ocrPage(doc, page, chosen))) break;
      processed.push(page);
    }
    running = false;
    progress = '';
    if (processed.length > 0) {
      // Une page sans mot reconnu n'a rien changé (aucune étape d'historique) : on la compte
      // à part plutôt que de l'annoncer « reconnue ».
      const after = (await pagesWithText(doc)) ?? [];
      const { recognized, empty } = ocrOutcome(processed, after);
      showNotice(
        empty > 0 ? `${t.ocrDone(recognized)} · ${t.ocrEmpty(empty)}` : t.ocrDone(recognized),
      );
    }
    onclose();
  }
</script>

<div
  class="dialog-backdrop"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && !running && onclose()}
>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="ocr-title" use:trapFocus>
    <h2 id="ocr-title">{t.ocrTitle}</h2>
    <p class="intro">{t.ocrIntro}</p>
    {#if status && !status.available}
      <p class="missing" role="alert">{t.ocrMissing}</p>
    {:else if status}
      <fieldset>
        <legend>{t.ocrLanguages}</legend>
        {#each status.languages as lang (lang)}
          <label class="check">
            <input
              type="checkbox"
              checked={chosen.includes(lang)}
              disabled={running}
              onchange={(e) => toggle(lang, e.currentTarget.checked)}
            />
            {lang}
          </label>
        {/each}
      </fieldset>
      <fieldset>
        <legend>{t.pageScope}</legend>
        <label class="check">
          <input
            type="radio"
            name="ocr-scope"
            value="untexted"
            bind:group={scope}
            disabled={running}
          />
          {t.ocrScopeUntexted}
        </label>
        <label class="check">
          <input
            type="radio"
            name="ocr-scope"
            value="current"
            bind:group={scope}
            disabled={running}
          />
          {t.ocrScopeCurrent}
        </label>
      </fieldset>
    {/if}
    {#if progress}
      <p role="status">{progress}</p>
    {/if}
    {#if message}
      <p class="missing" role="alert">{message}</p>
    {/if}
    <div class="actions">
      {#if running}
        <button type="button" onclick={() => (stopRequested = true)}>{t.ocrStop}</button>
      {:else}
        <button type="button" onclick={onclose}>{t.cancel}</button>
        <button type="button" class="primary" disabled={!status?.available} onclick={run}
          >{t.ocrRun}</button
        >
      {/if}
    </div>
  </div>
</div>

<style>
  .intro {
    max-width: 30rem;
    line-height: 1.4;
  }
  fieldset {
    border: 1px solid var(--panel-border);
    border-radius: 6px;
    margin: 0.5rem 0;
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem 1rem;
  }
  .check {
    display: flex;
    gap: 0.35rem;
    align-items: center;
  }
  .missing {
    color: var(--danger-text);
    background: var(--danger-bg);
    border-radius: 6px;
    padding: 0.4rem 0.6rem;
    max-width: 30rem;
  }
</style>
