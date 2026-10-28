<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  // Préférences (Fichier ▸ Préférences…, Ctrl+,) : chaque réglage s'applique et s'enregistre
  // immédiatement ; il n'y a donc qu'un bouton « Fermer ».
  import { trapFocus } from '$lib/actions/trapFocus';
  import { pickExecutable, setTesseractPath } from '$lib/api';
  import type { LanguagePreference } from '$lib/logic/i18n';
  import {
    clearRecent,
    saveSettings,
    setLanguagePreference,
    settings,
    type Theme,
    type ViewMode,
    type ZoomMode,
  } from '$lib/state/settings.svelte';
  import { t } from '$lib/strings';

  let { onclose }: { onclose: () => void } = $props();

  type Simple = 'theme' | 'defaultZoomMode' | 'viewMode' | 'author' | 'restorePositions';

  /** Chemin de Tesseract : enregistré et transmis au cœur (vide : recherche automatique). */
  function setTesseract(path: string) {
    settings.tesseractPath = path.trim();
    saveSettings();
    void setTesseractPath(settings.tesseractPath || null).catch(() => {});
  }

  async function browseTesseract() {
    const picked = await pickExecutable().catch(() => null);
    if (picked) setTesseract(picked);
  }

  /** Modifie un réglage simple puis l'enregistre (sauvegarde différée). */
  function set<K extends Simple>(key: K, value: (typeof settings)[K]) {
    settings[key] = value;
    saveSettings();
  }
</script>

<div
  class="dialog-backdrop"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && onclose()}
>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="prefs-title" use:trapFocus>
    <h2 id="prefs-title">{t.preferencesTitle}</h2>
    <div class="grid">
      <label for="pref-language">{t.prefLanguage}</label>
      <select
        id="pref-language"
        value={settings.language}
        onchange={(e) => setLanguagePreference(e.currentTarget.value as LanguagePreference)}
      >
        <option value="system">{t.languageSystem}</option>
        <option value="fr" lang="fr">{t.languageFr}</option>
        <option value="en" lang="en">{t.languageEn}</option>
      </select>

      <label for="pref-theme">{t.theme}</label>
      <select
        id="pref-theme"
        value={settings.theme}
        onchange={(e) => set('theme', e.currentTarget.value as Theme)}
      >
        <option value="system">{t.themeSystem}</option>
        <option value="light">{t.themeLight}</option>
        <option value="dark">{t.themeDark}</option>
      </select>

      <label for="pref-zoom">{t.prefZoom}</label>
      <select
        id="pref-zoom"
        value={settings.defaultZoomMode}
        onchange={(e) => set('defaultZoomMode', e.currentTarget.value as ZoomMode)}
      >
        <option value="fitWidth">{t.fitWidth}</option>
        <option value="fitPage">{t.fitPage}</option>
        <option value="custom">{t.prefZoomCustom}</option>
      </select>

      <label for="pref-view">{t.prefView}</label>
      <select
        id="pref-view"
        value={settings.viewMode}
        onchange={(e) => set('viewMode', e.currentTarget.value as ViewMode)}
      >
        <option value="continuous">{t.viewContinuous}</option>
        <option value="single">{t.viewSingle}</option>
      </select>

      <label for="pref-author">{t.prefAuthor}</label>
      <input
        id="pref-author"
        type="text"
        value={settings.author}
        oninput={(e) => set('author', e.currentTarget.value)}
      />

      <label for="pref-tesseract">{t.prefTesseract}</label>
      <span class="path">
        <input
          id="pref-tesseract"
          type="text"
          placeholder={t.prefTesseractAuto}
          value={settings.tesseractPath}
          onchange={(e) => setTesseract(e.currentTarget.value)}
        />
        <button type="button" onclick={browseTesseract}>{t.browse}</button>
      </span>
    </div>

    <label class="check">
      <input
        type="checkbox"
        checked={settings.restorePositions}
        onchange={(e) => set('restorePositions', e.currentTarget.checked)}
      />
      {t.prefRestore}
    </label>

    <p class="recent">
      <span>{t.prefRecent(settings.recent.length)}</span>
      <button type="button" disabled={settings.recent.length === 0} onclick={clearRecent}
        >{t.clearRecent}</button
      >
    </p>

    <div class="actions">
      <button type="button" class="primary" onclick={onclose}>{t.close}</button>
    </div>
  </div>
</div>

<style>
  .path {
    display: flex;
    gap: 0.4rem;
  }
  .path input {
    flex: 1;
    min-width: 0;
  }
  .grid {
    display: grid;
    grid-template-columns: max-content minmax(14rem, 1fr);
    gap: 0.5rem 0.75rem;
    align-items: center;
  }
  .check {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    margin-top: 0.75rem;
  }
  .recent {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
    margin: 0.75rem 0 0;
  }
</style>
