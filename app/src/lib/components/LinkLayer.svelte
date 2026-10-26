<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import type { PageLink } from '$lib/api';
  import { pdfRectToCss } from '$lib/logic/geometry';
  import { externalLinkPolicy } from '$lib/logic/links';
  import { ensureLinks, type OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';

  let {
    doc,
    index,
    onnavigate,
  }: { doc: OpenDoc; index: number; onnavigate?: (page: number) => void } = $props();

  const page = $derived(doc.summary.pages[index]);
  let links = $state<PageLink[]>([]);

  $effect(() => {
    let cancelled = false;
    ensureLinks(doc, index).then((l) => {
      if (!cancelled) links = l;
    });
    return () => {
      cancelled = true;
    };
  });

  async function activate(link: PageLink) {
    if (link.target.kind === 'page') {
      onnavigate?.(link.target.value);
      return;
    }
    const uri = link.target.value;
    if (externalLinkPolicy(uri) !== 'open') {
      const { message } = await import('@tauri-apps/plugin-dialog');
      await message(t.externalLinkBlocked, { kind: 'warning' }).catch(() => undefined);
      return;
    }
    const { confirm } = await import('@tauri-apps/plugin-dialog');
    const ok = await confirm(t.externalLinkConfirm(uri), { kind: 'info' }).catch(() => false);
    if (!ok) return;
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    await openUrl(uri).catch(() => undefined);
  }

  function title(link: PageLink): string {
    return link.target.kind === 'page'
      ? t.pageOf(link.target.value + 1, doc.summary.page_count)
      : link.target.value;
  }
</script>

{#each links as link, i (i)}
  {@const css = pdfRectToCss(link.rect, page, doc.scale, doc.rotation)}
  <button
    type="button"
    class="link"
    class:blocked={link.target.kind === 'uri' && externalLinkPolicy(link.target.value) !== 'open'}
    style:left="{css.left}px"
    style:top="{css.top}px"
    style:width="{css.width}px"
    style:height="{css.height}px"
    title={title(link)}
    aria-label={title(link)}
    onclick={() => activate(link)}
  ></button>
{/each}

<style>
  .link {
    position: absolute;
    padding: 0;
    border: none;
    border-radius: 2px;
    background: transparent;
    cursor: pointer;
    z-index: 2;
  }
  .link:hover,
  .link:focus-visible {
    background: var(--accent-soft);
  }
  .link.blocked {
    cursor: not-allowed;
  }
</style>
