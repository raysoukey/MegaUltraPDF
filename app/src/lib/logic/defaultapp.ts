// SPDX-License-Identifier: GPL-3.0-or-later
// Ce que les préférences disent du lecteur PDF par défaut (logique pure, testée).
import type { DefaultAppAction, DefaultAppStatus } from '$lib/api';
import { t } from '$lib/strings';

/** Texte de l'état, bouton disponible ou non, message après l'action. */
export interface DefaultAppView {
  status: string;
  canAct: boolean;
  message: string | null;
}

/**
 * Vue de la section « Lecteur PDF par défaut » : l'état lu par le système et, s'il y a eu
 * un clic, ce que l'action a fait (Windows ouvre ses Paramètres : c'est l'utilisateur qui
 * choisit MegaUltraPDF ; Linux le règle directement ; macOS demande le Finder).
 */
export function defaultAppView(
  status: DefaultAppStatus,
  action: DefaultAppAction | null,
): DefaultAppView {
  const message = action === null ? null : ACTION_MESSAGES[action]();
  if (status.portable) return { status: t.defaultAppPortable, canAct: false, message };
  switch (status.kind) {
    case 'default':
      return { status: t.defaultAppIsDefault, canAct: false, message };
    case 'other':
      return { status: t.defaultAppOther, canAct: true, message };
    case 'unknown':
      return { status: t.defaultAppUnknown, canAct: true, message };
  }
}

const ACTION_MESSAGES: Record<DefaultAppAction, () => string> = {
  opened_settings: () => t.defaultAppOpenedSettings,
  set: () => t.defaultAppSet,
  instructions: () => t.defaultAppInstructions,
  unsupported: () => t.defaultAppUnsupported,
};
