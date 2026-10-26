<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import type { PageSize } from '$lib/api';
  import { pageUrl } from '$lib/pageUrl';
  import { t } from '$lib/strings';

  let {
    docId,
    index,
    scale,
    size,
    total,
  }: { docId: number; index: number; scale: number; size: PageSize; total: number } = $props();

  // Dimensions CSS fixées d'avance : la page garde sa place pendant le chargement.
  const width = $derived(Math.round(size.width_pt * scale));
  const height = $derived(Math.round(size.height_pt * scale));
</script>

<img
  class="page"
  src={pageUrl(docId, index, scale)}
  alt={t.pageOf(index + 1, total)}
  {width}
  {height}
  draggable="false"
/>

<style>
  .page {
    display: block;
    background: #fff;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.25);
  }
</style>
