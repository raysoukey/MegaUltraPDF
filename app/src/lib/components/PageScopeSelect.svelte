<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import type { PageScope } from '$lib/logic/content';
  import type { OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';

  let { doc, value = $bindable() }: { doc: OpenDoc; value: PageScope } = $props();
</script>

<label class="row">
  {t.pageScope}
  <select bind:value>
    <option value="all">{t.scopeAll}</option>
    <option value="current">{t.scopeCurrent(doc.page + 1)}</option>
    <option value="selected" disabled={doc.selectedPages.length === 0}
      >{t.scopeSelected(doc.selectedPages.length)}</option
    >
  </select>
</label>

<style>
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    margin: 0.4rem 0;
  }
</style>
