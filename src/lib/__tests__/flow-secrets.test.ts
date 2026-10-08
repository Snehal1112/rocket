import { describe, expect, it } from 'vitest';
import type { Auth } from '@/lib/tauri-api';
import { isVariableReference, plaintextSecretFields, redactPlaintextSecrets } from '../flow-secrets';

const as = (auth: unknown) => auth as Auth;

describe('isVariableReference', () => {
  it.each(['{{token}}', ' {{ token }} ', '{{a}}{{b}}', '{{$guid}}', '{{vault.secret}}'])(
    'accepts %j',
    (value) => {
      expect(isVariableReference(value)).toBe(true);
    },
  );

  it.each(['', '   ', 'abc', 'Bearer {{token}}', '{{a}', 'x{{a}}', '{{a}} y', '{{}}'])(
    'rejects %j',
    (value) => {
      expect(isVariableReference(value)).toBe(false);
    },
  );
});

describe('plaintextSecretFields', () => {
  it('flags a literal bearer token only', () => {
    expect(plaintextSecretFields({ authType: 'bearer', token: 'abc123' })).toEqual(['Token']);
    expect(plaintextSecretFields({ authType: 'bearer', token: '{{token}}' })).toEqual([]);
    expect(plaintextSecretFields({ authType: 'bearer', token: '' })).toEqual([]);
    expect(plaintextSecretFields({ authType: 'bearer', token: '   ' })).toEqual([]);
  });

  it('does not flag the username, only the password', () => {
    const auth: Auth = { authType: 'basic', username: 'alice', password: 'hunter2' };
    expect(plaintextSecretFields(auth)).toEqual(['Password']);
    expect(plaintextSecretFields({ ...auth, password: '{{pw}}' })).toEqual([]);
  });

  it.each(['digest', 'wsse'] as const)('flags the %s password', (authType) => {
    expect(plaintextSecretFields({ authType, username: 'u', password: 'p4ss' })).toEqual([
      'Password',
    ]);
  });

  it('flags the NTLM password but not the domain', () => {
    expect(
      plaintextSecretFields({ authType: 'ntlm', username: 'u', password: 'p4ss', domain: 'corp' }),
    ).toEqual(['Password']);
  });

  it('flags the API key value but not its name', () => {
    expect(
      plaintextSecretFields({ authType: 'api-key', key: 'X-Key', value: 'k-123', placement: 'header' }),
    ).toEqual(['API key value']);
  });

  it('flags OAuth 2.0 client secret and resource owner password', () => {
    const auth = as({
      authType: 'o-auth2',
      flow: 'resource_owner_password_credentials',
      credentials: { clientId: 'cid', clientSecret: 'shh' },
      resourceOwner: { username: 'u', password: 'pw' },
    });
    expect(plaintextSecretFields(auth)).toEqual(['Client secret', 'Resource owner password']);
  });

  it('does not flag an OAuth 2.0 field that is a variable, or a token held in memory', () => {
    const auth = as({
      authType: 'o-auth2',
      flow: 'client_credentials',
      credentials: { clientId: 'cid', clientSecret: '{{secret}}' },
      accessToken: 'in-memory-token',
      refreshToken: 'in-memory-refresh',
    });
    expect(plaintextSecretFields(auth)).toEqual([]);
  });

  it('flags the AWS secret key and session token', () => {
    const auth = as({
      authType: 'aws-sig-v4',
      accessKey: 'AKIA',
      secretKey: 'sk',
      sessionToken: 'st',
      region: 'us-east-1',
      service: 's3',
    });
    expect(plaintextSecretFields(auth)).toEqual(['Secret access key', 'Session token']);
  });

  it('flags OAuth 1.0 secrets, and a private key only when it is inline text', () => {
    const base = {
      authType: 'o-auth1',
      consumerKey: 'ck',
      consumerSecret: 'cs',
      accessToken: 'at',
      accessTokenSecret: 'ats',
    };
    expect(plaintextSecretFields(as(base))).toEqual([
      'Consumer secret',
      'Access token',
      'Access token secret',
    ]);
    expect(
      plaintextSecretFields(as({ ...base, privateKey: { type: 'text', value: '-----BEGIN' } })),
    ).toContain('Private key');
    expect(
      plaintextSecretFields(as({ ...base, privateKey: { type: 'file', value: '/keys/a.pem' } })),
    ).not.toContain('Private key');
  });

  it('returns nothing for none and inherit', () => {
    expect(plaintextSecretFields({ authType: 'none' })).toEqual([]);
    expect(plaintextSecretFields({ authType: 'inherit' })).toEqual([]);
  });

  it('never returns a value', () => {
    const labels = plaintextSecretFields({ authType: 'bearer', token: 'super-secret-value' });
    expect(JSON.stringify(labels)).not.toContain('super-secret-value');
  });
});

describe('redactPlaintextSecrets', () => {
  it('replaces literal secrets and keeps variable references', () => {
    const auth = as({
      authType: 'o-auth2',
      flow: 'resource_owner_password_credentials',
      credentials: { clientId: 'cid', clientSecret: 'shh' },
      resourceOwner: { username: 'u', password: '{{pw}}' },
    });
    const out = redactPlaintextSecrets(auth, '<redacted>');
    expect(out.fields).toEqual(['Client secret']);
    expect(out.auth).toEqual({
      authType: 'o-auth2',
      flow: 'resource_owner_password_credentials',
      credentials: { clientId: 'cid', clientSecret: '<redacted>' },
      resourceOwner: { username: 'u', password: '{{pw}}' },
    });
  });

  it('does not change its input', () => {
    const auth: Auth = { authType: 'basic', username: 'u', password: 'p4ss' };
    redactPlaintextSecrets(auth, '<redacted>');
    expect(auth).toEqual({ authType: 'basic', username: 'u', password: 'p4ss' });
  });

  it('redacts an inline OAuth 1.0 private key but leaves a key file path', () => {
    const inline = as({ authType: 'o-auth1', privateKey: { type: 'text', value: 'PEM' } });
    expect(redactPlaintextSecrets(inline, 'X').auth).toEqual({
      authType: 'o-auth1',
      privateKey: { type: 'text', value: 'X' },
    });
    const file = as({ authType: 'o-auth1', privateKey: { type: 'file', value: '/k.pem' } });
    expect(redactPlaintextSecrets(file, 'X').auth).toEqual(file);
  });

  it('returns the same fields as plaintextSecretFields', () => {
    const auth: Auth = { authType: 'digest', username: 'u', password: 'p4ss' };
    expect(redactPlaintextSecrets(auth, 'X').fields).toEqual(plaintextSecretFields(auth));
  });
});
