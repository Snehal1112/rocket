// src/lib/vault-certificates.test.ts

import { type as osType } from '@tauri-apps/plugin-os';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { VaultCertificateSummary } from '@/lib/tauri-api';
import {
  isEcKeyAlgorithm,
  isSelectable,
  isWindows,
  needsEcPemWarning,
  vaultCertificateLabel,
} from '@/lib/vault-certificates';

vi.mock('@tauri-apps/plugin-os', () => ({ type: vi.fn(() => 'linux') }));

function summary(overrides: Partial<VaultCertificateSummary> = {}): VaultCertificateSummary {
  return {
    id: '1',
    name: 'client-a',
    exportable: true,
    enabled: true,
    keyAlgorithm: 'RSA-2048',
    expiresAt: null,
    ...overrides,
  };
}

describe('isEcKeyAlgorithm', () => {
  it('is true for the EC spellings RocketVault may use', () => {
    for (const alg of ['EC', 'ECDSA', 'EC-P256', ' ec-p384 ']) {
      expect(isEcKeyAlgorithm(alg)).toBe(true);
    }
  });

  it('is false for RSA, Ed25519 and a missing value', () => {
    for (const alg of ['RSA-2048', 'Ed25519', '', undefined]) {
      expect(isEcKeyAlgorithm(alg)).toBe(false);
    }
  });
});

describe('isWindows', () => {
  beforeEach(() => {
    vi.mocked(osType).mockReset();
  });

  it('follows the Tauri OS type', () => {
    vi.mocked(osType).mockReturnValue('windows');
    expect(isWindows()).toBe(true);
    vi.mocked(osType).mockReturnValue('linux');
    expect(isWindows()).toBe(false);
  });

  it('is false outside Tauri', () => {
    vi.mocked(osType).mockImplementation(() => {
      throw new Error('not running in Tauri');
    });
    expect(isWindows()).toBe(false);
  });
});

describe('needsEcPemWarning', () => {
  it('warns only for an EC key exported as PEM on Windows', () => {
    expect(needsEcPemWarning('EC-P256', 'pem', true)).toBe(true);
    expect(needsEcPemWarning('EC-P256', 'pkcs12', true)).toBe(false);
    expect(needsEcPemWarning('EC-P256', 'pem', false)).toBe(false);
    expect(needsEcPemWarning('RSA-2048', 'pem', true)).toBe(false);
    expect(needsEcPemWarning(undefined, 'pem', true)).toBe(false);
  });
});

describe('vaultCertificateLabel and isSelectable', () => {
  it('shows the name and key algorithm, and why a certificate cannot be picked', () => {
    expect(vaultCertificateLabel(summary())).toBe('client-a · RSA-2048');
    expect(vaultCertificateLabel(summary({ name: 'locked', exportable: false }))).toBe(
      'locked · RSA-2048 · not exportable',
    );
    expect(vaultCertificateLabel(summary({ name: 'old', enabled: false }))).toBe(
      'old · RSA-2048 · disabled',
    );
    expect(vaultCertificateLabel(summary({ name: 'bare', keyAlgorithm: '' }))).toBe('bare');
  });

  it('allows only exportable, enabled certificates', () => {
    expect(isSelectable(summary())).toBe(true);
    expect(isSelectable(summary({ exportable: false }))).toBe(false);
    expect(isSelectable(summary({ enabled: false }))).toBe(false);
  });
});
