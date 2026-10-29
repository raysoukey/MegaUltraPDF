<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  // Icône d'action (Lot 12), purement décorative : un lecteur d'écran l'ignore, le nom du
  // bouton vient de son `aria-label` et l'infobulle de son `title` (ADR-0012).
  import { iconSrc, type IconName } from '$lib/logic/icons';

  let { name, size = 24 }: { name: IconName; size?: number } = $props();
</script>

<img
  class="icon"
  src={iconSrc(name)}
  alt=""
  aria-hidden="true"
  draggable="false"
  width={size}
  height={size}
/>

<style>
  .icon {
    display: block;
    flex: none;
    pointer-events: none;
  }
  /* Un bouton désactivé est déjà estompé (app.css) ; l'icône perd aussi sa couleur. */
  :global(button:disabled) .icon {
    filter: grayscale(1);
  }
</style>
