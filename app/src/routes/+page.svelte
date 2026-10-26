<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { t } from '$lib/strings';
</script>

<main>
  <header>
    <h1>{t.appName}</h1>
    <button type="button">{t.openFile}</button>
  </header>
  <section class="viewer">
    <p>{t.noDocument}</p>
  </section>
</main>

<style>
  :global(body) {
    margin: 0;
    font-family: system-ui, sans-serif;
    background: #e9e9ec;
    color: #1d1d1f;
  }
  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  header {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0.5rem 1rem;
    background: #f7f7f8;
    border-bottom: 1px solid #d0d0d5;
  }
  h1 {
    margin: 0;
    font-size: 1rem;
    font-weight: 600;
  }
  .viewer {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: auto;
  }
</style>
