// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import type { SignatureInfo } from '$lib/api';
import { fr } from '$lib/strings';
import { signatureVerdict } from './signatures';

function info(patch: Partial<SignatureInfo>): SignatureInfo {
  return {
    field: 'Signature1',
    signer: 'Alice',
    signed_at: 1_800_000_000,
    reason: null,
    location: null,
    sub_filter: 'adbe.pkcs7.detached',
    status: { kind: 'intact' },
    covers_whole_document: true,
    ...patch,
  };
}

describe('signatureVerdict', () => {
  it('intact and covering the whole document is good, identity still not certified', () => {
    expect(signatureVerdict(info({}))).toEqual({ tone: 'ok', text: fr.sigIntact });
  });
  it('intact but followed by changes is a warning', () => {
    expect(signatureVerdict(info({ covers_whole_document: false }))).toEqual({
      tone: 'warn',
      text: fr.sigModifiedAfter,
    });
  });
  it('invalid shows the translated reason of its issue code', () => {
    expect(
      signatureVerdict(info({ status: { kind: 'invalid', issue: 'content_modified' } })),
    ).toEqual({
      tone: 'bad',
      text: fr.sigInvalid(fr.sigIssues.content_modified),
    });
  });
  it('unsupported is a warning with the translated reason', () => {
    expect(
      signatureVerdict(info({ status: { kind: 'unsupported', issue: 'digest_algorithm' } })),
    ).toEqual({
      tone: 'warn',
      text: fr.sigUnsupported(fr.sigIssues.digest_algorithm),
    });
  });
});
