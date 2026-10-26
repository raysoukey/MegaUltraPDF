// SPDX-License-Identifier: GPL-3.0-or-later
// Application du thème : attribut `data-theme` sur <html>, lu par les variables CSS.

import type { Theme } from './state/settings.svelte';

export function applyTheme(theme: Theme): void {
  if (typeof document === 'undefined') return;
  const root = document.documentElement;
  if (theme === 'system') {
    root.removeAttribute('data-theme');
  } else {
    root.setAttribute('data-theme', theme);
  }
}
