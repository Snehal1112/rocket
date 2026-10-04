import { describe, expect, it } from 'vitest';
import {
  bindingScopeColumnLabel,
  canAddVaultCertificate,
  getProviderDescriptor,
  SECRET_PROVIDERS,
} from '@/lib/secret-providers';
import type { ExternalSecretBinding, SecretManagerConnection } from '@/lib/tauri-api';

const rocketVault: SecretManagerConnection = {
  id: 'rv',
  label: 'Prod RocketVault',
  baseUrl: 'https://vault.internal:8774',
  clientId: 'rocketapi',
  verifySsl: true,
  allowInsecureHttp: false,
};

const azure: SecretManagerConnection = { ...rocketVault, id: 'az', provider: 'azure' };

function binding(connectionId: string): ExternalSecretBinding {
  return { alias: `a-${connectionId}`, connectionId, vaultName: 'v', secretNames: [] };
}

describe('getProviderDescriptor', () => {
  it('treats a missing provider as RocketVault', () => {
    expect(getProviderDescriptor(undefined).kind).toBe('rocketvault');
    expect(getProviderDescriptor(null).kind).toBe('rocketvault');
  });

  it('only RocketVault is selectable and supports certificates', () => {
    const selectable = SECRET_PROVIDERS.filter((p) => p.selectable).map((p) => p.kind);
    const certificates = SECRET_PROVIDERS.filter((p) => p.supportsCertificates).map((p) => p.kind);
    expect(selectable).toEqual(['rocketvault']);
    expect(certificates).toEqual(['rocketvault']);
  });

  it('lists the RocketVault connection fields the form shows today', () => {
    expect(getProviderDescriptor('rocketvault').connectionFields).toEqual([
      'baseUrl',
      'clientId',
      'clientSecret',
      'verifySsl',
      'allowInsecureHttp',
    ]);
  });
});

describe('bindingScopeColumnLabel', () => {
  it('says Vault Name when there are no bindings or only RocketVault ones', () => {
    expect(bindingScopeColumnLabel([], [rocketVault])).toBe('Vault Name');
    expect(bindingScopeColumnLabel([binding('rv')], [rocketVault])).toBe('Vault Name');
  });

  it('says Scope when a binding uses another provider', () => {
    expect(bindingScopeColumnLabel([binding('rv'), binding('az')], [rocketVault, azure])).toBe(
      'Scope',
    );
  });
});

describe('canAddVaultCertificate', () => {
  it('stays true while connections are still loading', () => {
    expect(canAddVaultCertificate([], [], false)).toBe(true);
  });

  it('is false with no binding once connections have loaded', () => {
    expect(canAddVaultCertificate([], [rocketVault], true)).toBe(false);
  });

  it('is true when a binding points at a RocketVault connection', () => {
    expect(canAddVaultCertificate([binding('rv')], [rocketVault], true)).toBe(true);
  });

  it('is false when every binding points at a provider without certificates', () => {
    expect(canAddVaultCertificate([binding('az')], [azure], true)).toBe(false);
  });

  it('ignores a binding whose connection was deleted', () => {
    expect(canAddVaultCertificate([binding('gone')], [rocketVault], true)).toBe(false);
  });
});
