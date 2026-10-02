// src/lib/vault-secret-options.test.ts

import { describe, expect, it } from 'vitest';
import type { ExternalSecretBinding } from '@/lib/tauri-api';
import { vaultSecretOptions } from '@/lib/vault-secret-options';

function binding(alias: string, names: string[]): ExternalSecretBinding {
  return {
    alias,
    connectionId: 'conn-1',
    vaultName: 'prod-vault',
    secretNames: names.map((name, i) => ({ name, secretId: `${alias}-${i}` })),
  };
}

describe('vaultSecretOptions', () => {
  it('lists every bound secret as alias.secretName in binding order', () => {
    expect(
      vaultSecretOptions([binding('vault', ['clientCert', 'clientKey']), binding('other', ['pw'])]),
    ).toEqual([
      { value: 'vault.clientCert', label: 'vault.clientCert' },
      { value: 'vault.clientKey', label: 'vault.clientKey' },
      { value: 'other.pw', label: 'other.pw' },
    ]);
  });

  it('skips a binding with no alias and a binding with no fetched secrets', () => {
    expect(vaultSecretOptions([binding('', ['a']), binding('vault', [])])).toEqual([]);
  });

  it('returns an empty list for no bindings', () => {
    expect(vaultSecretOptions([])).toEqual([]);
  });
});
