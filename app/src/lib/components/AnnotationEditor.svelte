<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import type { Annotation, Color } from '$lib/api';
  import { toNewAnnotation } from '$lib/logic/annotations';
  import { updateAnnotation, type OpenDoc } from '$lib/state/documents.svelte';
  import { t } from '$lib/strings';

  let {
    doc,
    page,
    annotation,
    onclose,
  }: { doc: OpenDoc; page: number; annotation: Annotation; onclose: () => void } = $props();

  // svelte-ignore state_referenced_locally
  const base = toNewAnnotation(annotation);
  // svelte-ignore state_referenced_locally
  let contents = $state(annotation.contents ?? '');
  // svelte-ignore state_referenced_locally
  let author = $state(annotation.author ?? '');
  let color = $state(hex(base?.style.color ?? { r: 0, g: 0, b: 0 }));
  let opacity = $state(base?.style.opacity ?? 1);
  let strokeWidth = $state(base?.style.stroke_width ?? 1);
  let fontSize = $state(base?.style.font_size ?? 12);
  let busy = $state(false);
  let first: HTMLTextAreaElement | undefined = $state();

  $effect(() => {
    first?.focus();
  });

  const isShape = $derived(['rect', 'ellipse', 'line', 'arrow', 'ink'].includes(annotation.kind));
  const isText = $derived(annotation.kind === 'text' || annotation.kind === 'stamp');
  const hasContents = $derived(
    annotation.kind === 'note' || annotation.kind === 'text' || annotation.kind === 'stamp',
  );

  function hex(c: Color): string {
    return '#' + [c.r, c.g, c.b].map((v) => v.toString(16).padStart(2, '0')).join('');
  }
  function fromHex(h: string): Color {
    return {
      r: Number.parseInt(h.slice(1, 3), 16),
      g: Number.parseInt(h.slice(3, 5), 16),
      b: Number.parseInt(h.slice(5, 7), 16),
    };
  }

  async function submit(e: Event) {
    e.preventDefault();
    if (!base) return;
    busy = true;
    const ok = await updateAnnotation(doc, page, annotation.index, {
      ...base,
      contents,
      author,
      style: {
        ...base.style,
        color: fromHex(color),
        opacity,
        stroke_width: strokeWidth,
        font_size: fontSize,
      },
    });
    busy = false;
    if (ok) onclose();
  }
</script>

<div
  class="dialog-backdrop"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && onclose()}
>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="ann-title">
    <form onsubmit={submit}>
      <h2 id="ann-title">
        {t.editAnnotation} — {t.annotationKind[annotation.kind] ?? annotation.kind}
      </h2>
      {#if !base}
        <p>{t.annotationNotEditable}</p>
      {:else}
        {#if hasContents}
          <label>
            {t.annotationContents}
            <textarea bind:this={first} bind:value={contents} rows="4"></textarea>
          </label>
        {/if}
        <div class="row">
          <label>
            {t.color}
            <input type="color" bind:value={color} />
          </label>
          <label>
            {t.opacity}
            <input type="range" min="0.1" max="1" step="0.1" bind:value={opacity} />
          </label>
          {#if isShape}
            <label>
              {t.strokeWidth}
              <input type="number" min="1" max="24" bind:value={strokeWidth} />
            </label>
          {/if}
          {#if isText}
            <label>
              {t.fontSize}
              <input type="number" min="6" max="72" bind:value={fontSize} />
            </label>
          {/if}
        </div>
        <label>
          {t.author}
          <input type="text" bind:value={author} />
        </label>
      {/if}
      <div class="actions">
        <button type="button" onclick={onclose}>{t.cancel}</button>
        <button type="submit" class="primary" disabled={!base || busy}>{t.ok}</button>
      </div>
    </form>
  </div>
</div>

<style>
  label {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.85rem;
    margin: 0.4rem 0;
  }
  .row {
    display: flex;
    gap: 0.75rem;
    flex-wrap: wrap;
  }
  textarea {
    font: inherit;
    min-width: 280px;
  }
  input[type='number'] {
    width: 4rem;
  }
</style>
