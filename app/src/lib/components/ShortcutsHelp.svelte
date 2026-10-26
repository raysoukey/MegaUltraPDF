<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { SHORTCUT_HELP } from '$lib/logic/shortcuts';
  import { t } from '$lib/strings';

  let { onclose }: { onclose: () => void } = $props();
</script>

<div
  class="dialog-backdrop"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && onclose()}
>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="help-title">
    <h2 id="help-title">{t.shortcutsHelp}</h2>
    <table>
      <tbody>
        {#each SHORTCUT_HELP as row (row.action)}
          <tr>
            <td class="keys"
              >{#each row.keys as k, i (k)}{#if i > 0},
                {/if}<kbd>{k}</kbd>{/each}</td
            >
            <td>{row.label}</td>
          </tr>
        {/each}
        <tr><td class="keys"><kbd>Ctrl+molette</kbd></td><td>{t.zoomIn} / {t.zoomOut}</td></tr>
        <tr><td class="keys"><kbd>Ctrl+C</kbd></td><td>{t.copy}</td></tr>
      </tbody>
    </table>
    <div class="actions">
      <button type="button" class="primary" onclick={onclose}>{t.close}</button>
    </div>
  </div>
</div>

<style>
  table {
    border-collapse: collapse;
  }
  td {
    padding: 0.2rem 1rem 0.2rem 0;
  }
  .keys {
    white-space: nowrap;
  }
  kbd {
    background: var(--bg);
    border: 1px solid var(--panel-border);
    border-radius: 4px;
    padding: 0 0.35rem;
    font-family: inherit;
  }
</style>
