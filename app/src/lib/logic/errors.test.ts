// SPDX-License-Identifier: GPL-3.0-or-later
import { afterEach, describe, expect, it } from 'vitest';
import { en, fr, setLanguage } from '$lib/strings';
import { openErrorMessage } from './errors';

const french = "Erreur d'accès au fichier : Le fichier spécifié est introuvable. (os error 2)";

describe('openErrorMessage', () => {
  afterEach(() => setLanguage('fr'));
  it('translates the known kinds instead of showing the French core message', () => {
    setLanguage('en');
    expect(openErrorMessage({ kind: 'io', message: french })).toBe(en.openError(en.errorKinds.io));
    expect(openErrorMessage({ kind: 'invalid_pdf', message: 'Fichier PDF invalide' })).toBe(
      en.openError(en.errorKinds.invalid_pdf),
    );
  });
  it('keeps the detailed message for the other kinds', () => {
    setLanguage('en');
    expect(openErrorMessage({ kind: 'internal', message: 'détail' })).toBe(en.openError('détail'));
  });
  it('follows the current language', () => {
    expect(openErrorMessage({ kind: 'io', message: french })).toBe(fr.openError(fr.errorKinds.io));
  });
});
