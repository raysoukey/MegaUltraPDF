// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { fr } from '$lib/strings';
import { defaultAppView } from './defaultapp';

describe('defaultAppView', () => {
  it('says MegaUltraPDF is the default and hides the button', () => {
    const v = defaultAppView(
      { kind: 'default', handler: 'MegaUltraPDF.Document', portable: false },
      null,
    );
    expect(v).toEqual({ status: fr.defaultAppIsDefault, canAct: false, message: null });
  });
  it('offers the button when another program opens PDFs', () => {
    const v = defaultAppView({ kind: 'other', handler: 'MSEdgePDF', portable: false }, null);
    expect(v.status).toBe(fr.defaultAppOther);
    expect(v.canAct).toBe(true);
  });
  it('portable builds cannot be associated', () => {
    const v = defaultAppView({ kind: 'other', handler: null, portable: true }, null);
    expect(v).toEqual({ status: fr.defaultAppPortable, canAct: false, message: null });
  });
  it('explains what happened after the action', () => {
    const status = { kind: 'other' as const, handler: 'MSEdgePDF', portable: false };
    expect(defaultAppView(status, 'opened_settings').message).toBe(fr.defaultAppOpenedSettings);
    expect(defaultAppView(status, 'instructions').message).toBe(fr.defaultAppInstructions);
    expect(defaultAppView(status, 'unsupported').message).toBe(fr.defaultAppUnsupported);
    expect(defaultAppView(status, 'set').message).toBe(fr.defaultAppSet);
  });
  it('unknown state still offers the action', () => {
    const v = defaultAppView({ kind: 'unknown', handler: null, portable: false }, null);
    expect(v).toEqual({ status: fr.defaultAppUnknown, canAct: true, message: null });
  });
});
