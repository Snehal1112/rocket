import { describe, expect, it } from 'vitest';
import { buildResolver, sourceBadgeClass } from '../url-variables';

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
