// src/lib/certificate-validation.test.ts

import { describe, expect, it } from 'vitest';
import { isLiteralPassphrase, validateClientCertificates } from '@/lib/certificate-validation';
import type { ClientCertificate, ExternalSecretBinding } from '@/lib/tauri-api';

const bindings: ExternalSecretBinding[] = [
  {
    alias: 'vault',
    connectionId: 'conn-1',
    vaultName: 'prod-vault',
    secretNames: [
      { name: 'clientCertPem', secretId: '1' },
      { name: 'clientKeyPem', secretId: '2' },
      { name: 'bundleB64', secretId: '3' },
    ],
  },
];

const validPem: ClientCertificate = {
  type: 'pem',
  domain: 'api.example.com',
  certificateFilePath: 'certs/client.pem',
  privateKeyFilePath: '/etc/ssl/client.key',
};

describe('validateClientCertificates', () => {
  it('accepts file sources, vault sources and a placeholder passphrase', () => {
    const certs: ClientCertificate[] = [
      { ...validPem, passphrase: '{{vault.clientKeyPass}}' },
      {
        type: 'pem',
        domain: '*.example.com',
        certificateSecret: 'vault.clientCertPem',
        privateKeySecret: 'vault.clientKeyPem',
      },
      { type: 'pkcs12', domain: 'host:8443', pkcs12Secret: 'vault.bundleB64' },
    ];
    expect(validateClientCertificates(certs, bindings)).toEqual({ errors: [], warnings: [] });
  });

  it('accepts absolute, home and variable-prefixed paths', () => {
    const certs: ClientCertificate[] = [
      { type: 'pkcs12', domain: 'a.com', pkcs12FilePath: '/abs/../fine.p12' },
      { type: 'pkcs12', domain: 'b.com', pkcs12FilePath: '~/certs/b.p12' },
      { type: 'pkcs12', domain: 'c.com', pkcs12FilePath: '{{certDir}}/c.p12' },
    ];
    expect(validateClientCertificates(certs, bindings).errors).toEqual([]);
  });

  it('rejects an empty domain', () => {
    const { errors } = validateClientCertificates([{ ...validPem, domain: '  ' }], bindings);
    expect(errors).toEqual(['Certificate 1: domain is required.']);
  });

  it('rejects a piece with no source and a piece with both sources', () => {
    const certs: ClientCertificate[] = [
      { type: 'pem', domain: 'a.com', certificateFilePath: 'a.pem' },
      {
        type: 'pkcs12',
        domain: 'b.com',
        pkcs12FilePath: 'b.p12',
        pkcs12Secret: 'vault.bundleB64',
      },
    ];
    const { errors } = validateClientCertificates(certs, bindings);
    expect(errors).toEqual([
      'Certificate 1 (a.com): Private key needs a file path or a vault secret.',
      'Certificate 2 (b.com): PKCS12 bundle must have either a file path or a vault secret, not both.',
    ]);
  });

  it('treats an empty vault selection as no source', () => {
    const { errors } = validateClientCertificates(
      [{ type: 'pkcs12', domain: 'a.com', pkcs12FilePath: '', pkcs12Secret: '' }],
      bindings,
    );
    expect(errors).toEqual([
      'Certificate 1 (a.com): PKCS12 bundle needs a file path or a vault secret.',
    ]);
  });

  it('rejects a reference with no matching binding', () => {
    const certs: ClientCertificate[] = [
      { type: 'pkcs12', domain: 'a.com', pkcs12Secret: 'nope.bundleB64' },
      { type: 'pkcs12', domain: 'b.com', pkcs12Secret: 'vault.missing' },
      { type: 'pkcs12', domain: 'c.com', pkcs12Secret: 'noDot' },
    ];
    const { errors } = validateClientCertificates(certs, bindings);
    expect(errors).toEqual([
      'Certificate 1 (a.com): PKCS12 bundle secret "nope.bundleB64" has no External Secrets binding with alias "nope".',
      'Certificate 2 (b.com): PKCS12 bundle secret "vault.missing" is not one of the fetched secrets for alias "vault". Fetch the secrets on the External Secrets tab.',
      'Certificate 3 (c.com): PKCS12 bundle secret "noDot" must look like alias.secretName.',
    ]);
  });

  it('rejects a pasted private key in a path or reference field', () => {
    const certs: ClientCertificate[] = [
      {
        type: 'pem',
        domain: 'a.com',
        certificateFilePath: 'a.pem',
        privateKeyFilePath: '-----BEGIN PRIVATE KEY-----\nMIIE',
      },
      { type: 'pkcs12', domain: 'b.com', pkcs12Secret: '  -----BEGIN CERTIFICATE-----' },
    ];
    const { errors } = validateClientCertificates(certs, bindings);
    expect(errors).toEqual([
      'Certificate 1 (a.com): Private key file path must be a file path or a vault secret reference, not key text.',
      'Certificate 2 (b.com): PKCS12 bundle secret must be a file path or a vault secret reference, not key text.',
    ]);
  });

  it('rejects a relative path that contains ..', () => {
    const certs: ClientCertificate[] = [
      { type: 'pkcs12', domain: 'a.com', pkcs12FilePath: '../outside/a.p12' },
      { type: 'pkcs12', domain: 'b.com', pkcs12FilePath: 'certs\\..\\b.p12' },
    ];
    const { errors } = validateClientCertificates(certs, bindings);
    expect(errors).toEqual([
      'Certificate 1 (a.com): PKCS12 bundle file path must not contain "..".',
      'Certificate 2 (b.com): PKCS12 bundle file path must not contain "..".',
    ]);
  });

  it('warns, but does not error, on a literal passphrase', () => {
    const { errors, warnings } = validateClientCertificates(
      [{ ...validPem, passphrase: 'hunter2' }],
      bindings,
    );
    expect(errors).toEqual([]);
    expect(warnings).toEqual([
      'Certificate 1 (api.example.com): the passphrase is a literal and will be saved in the environment file. Use a vault secret placeholder such as {{vault.NAME}}.',
    ]);
  });

  it('does not warn for an empty passphrase or a placeholder', () => {
    expect(
      validateClientCertificates([{ ...validPem, passphrase: '' }], bindings).warnings,
    ).toEqual([]);
    expect(
      validateClientCertificates([{ ...validPem, passphrase: 'pre-{{vault.p}}' }], bindings)
        .warnings,
    ).toEqual([]);
  });
});

describe('isLiteralPassphrase', () => {
  it('is true only for a non-empty value with no {{...}} placeholder', () => {
    expect(isLiteralPassphrase(undefined)).toBe(false);
    expect(isLiteralPassphrase('')).toBe(false);
    expect(isLiteralPassphrase('{{vault.pass}}')).toBe(false);
    expect(isLiteralPassphrase('hunter2')).toBe(true);
  });
});
