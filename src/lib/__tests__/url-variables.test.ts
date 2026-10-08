import { describe, expect, it } from 'vitest';
import { buildResolver, buildScopedContext, secretKeysOf, sourceBadgeClass } from '../url-variables';

describe('buildResolver with dynamic variables', () => {
  it('resolves {{$guid}} to a valid UUID', () => {
    const resolve = buildResolver({});
    const result = resolve('{{$guid}}');
    expect(result).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i,
    );
  });

  it('resolves {{$randomUUID}} inside a URL template', () => {
    const resolve = buildResolver({});
    const result = resolve('https://api.test/users/{{$randomUUID}}');
    expect(result).toMatch(/^https:\/\/api\.test\/users\/[0-9a-f]{8}-/i);
  });

  it('leaves unknown $vars unresolved', () => {
    const resolve = buildResolver({});
    expect(resolve('{{$doesNotExist}}')).toBe('{{$doesNotExist}}');
  });

  it('does not shadow dynamic vars with user env vars', () => {
    // Even when the env map has a '$guid' key, the dynamic generator wins.
    const resolve = buildResolver({ $guid: 'user-override' });
    const result = resolve('{{$guid}}');
    expect(result).not.toBe('user-override');
    expect(result).toMatch(/^[0-9a-f]{8}-/i);
  });

  it('still resolves regular vars alongside dynamic vars', () => {
    const resolve = buildResolver({ baseUrl: 'https://api.test' });
    const result = resolve('{{baseUrl}}/users/{{$guid}}');
    expect(result).toMatch(/^https:\/\/api\.test\/users\/[0-9a-f]{8}-/i);
  });

  it('two $guid in same template produce different values', () => {
    const resolve = buildResolver({});
    const result = resolve('{{$guid}}|{{$guid}}');
    const [a, b] = result.split('|');
    expect(a).toMatch(/^[0-9a-f]{8}-/i);
    expect(b).toMatch(/^[0-9a-f]{8}-/i);
    expect(a).not.toBe(b);
  });
});

describe('sourceBadgeClass', () => {
  it('returns the cyan class for dynamic source', () => {
    const cls = sourceBadgeClass('dynamic');
    expect(cls).toContain('cyan');
  });
});

describe('secretKeysOf', () => {
  it('lists only enabled secret variables', () => {
    const keys = secretKeysOf([
      { key: 'apiKey', enabled: true, secret: true },
      { key: 'host', enabled: true, secret: false },
      { key: 'old', enabled: false, secret: true },
    ]);
    expect([...keys]).toEqual(['apiKey']);
  });

  it('accepts a missing list', () => {
    expect(secretKeysOf(undefined).size).toBe(0);
    expect(secretKeysOf(null).size).toBe(0);
  });
});

describe('buildScopedContext secret flag', () => {
  it('marks a secret environment variable as secret', () => {
    const ctx = buildScopedContext({
      envVars: { apiKey: 'sk-live-123', host: 'api.test' },
      envSecretKeys: new Set(['apiKey']),
    });
    expect(ctx.get('apiKey')?.secret).toBe(true);
    expect(ctx.get('apiKey')?.source).toBe('environment');
    expect(ctx.get('host')?.secret).toBe(false);
  });

  it('marks a secret global variable as secret', () => {
    const ctx = buildScopedContext({
      globalVars: { token: 'g-123' },
      globalSecretKeys: new Set(['token']),
    });
    expect(ctx.get('token')?.secret).toBe(true);
    expect(ctx.get('token')?.source).toBe('global');
  });

  it('keeps the real value on a secret entry so OAuth2 can still resolve it', () => {
    const ctx = buildScopedContext({
      envVars: { clientSecret: 'real-secret' },
      envSecretKeys: new Set(['clientSecret']),
    });
    expect(ctx.get('clientSecret')?.value).toBe('real-secret');
  });

  it('is unchanged when no secret keys are given', () => {
    const ctx = buildScopedContext({ envVars: { a: '1' }, globalVars: { b: '2' } });
    expect(ctx.get('a')?.secret).toBe(false);
    expect(ctx.get('b')?.secret).toBe(false);
  });

  it('does not let a higher non-secret layer inherit a lower secret flag', () => {
    const ctx = buildScopedContext({
      globalVars: { k: 'global-secret' },
      globalSecretKeys: new Set(['k']),
      envVars: { k: 'plain-env-value' },
    });
    // The environment layer wins and is not secret.
    expect(ctx.get('k')).toEqual(
      expect.objectContaining({ source: 'environment', value: 'plain-env-value', secret: false }),
    );
  });

  it('does not let a lower non-secret layer hide a higher secret flag', () => {
    const ctx = buildScopedContext({
      globalVars: { k: 'plain-global' },
      envVars: { k: 'env-secret' },
      envSecretKeys: new Set(['k']),
    });
    expect(ctx.get('k')).toEqual(
      expect.objectContaining({ source: 'environment', secret: true }),
    );
  });

  it('adds no entry for a variable that is not in the values', () => {
    const ctx = buildScopedContext({ envVars: {}, envSecretKeys: new Set(['ghost']) });
    expect(ctx.has('ghost')).toBe(false);
  });
});
