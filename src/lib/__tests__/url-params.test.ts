import { describe, it, expect } from 'vitest';
import {
  parseQueryParams,
  buildUrl,
  extractPathParams,
  splitUrl,
} from '../url-params';

// ─── splitUrl ────────────────────────────────────────────────────────────────

describe('splitUrl', () => {
  it('splits base and query string correctly', () => {
    const result = splitUrl('https://api.example.com/users?page=1&limit=10');
    expect(result.base).toBe('https://api.example.com/users');
    expect(result.queryString).toBe('page=1&limit=10');
  });

  it('returns empty queryString when no ? present', () => {
    const result = splitUrl('https://api.example.com/users');
    expect(result.base).toBe('https://api.example.com/users');
    expect(result.queryString).toBe('');
  });

  it('handles empty string input', () => {
    const result = splitUrl('');
    expect(result.base).toBe('');
    expect(result.queryString).toBe('');
  });

  it('uses only the first ? as the separator', () => {
    // Second ? is technically invalid but should still be handled predictably.
    const result = splitUrl('https://example.com/path?a=1&b=2');
    expect(result.base).toBe('https://example.com/path');
    expect(result.queryString).toBe('a=1&b=2');
  });
});

// ─── parseQueryParams ─────────────────────────────────────────────────────────

describe('parseQueryParams', () => {
  it('parses basic key-value pairs', () => {
    const entries = parseQueryParams('https://api.example.com/users?page=1&limit=10');
    expect(entries).toHaveLength(2);
    expect(entries[0]).toMatchObject({ key: 'page', value: '1', enabled: true });
    expect(entries[1]).toMatchObject({ key: 'limit', value: '10', enabled: true });
  });

  it('assigns a unique id to each entry', () => {
    const entries = parseQueryParams('https://example.com?a=1&b=2');
    expect(entries[0].id).toBeTruthy();
    expect(entries[1].id).toBeTruthy();
    expect(entries[0].id).not.toBe(entries[1].id);
  });

  it('returns an empty array for an empty URL string', () => {
    expect(parseQueryParams('')).toEqual([]);
  });

  it('returns an empty array when there are no query parameters', () => {
    expect(parseQueryParams('https://api.example.com/users')).toEqual([]);
  });

  it('decodes percent-encoded characters in keys and values', () => {
    const entries = parseQueryParams(
      'https://example.com?q=hello%20world&tag=foo%2Bbar'
    );
    expect(entries[0]).toMatchObject({ key: 'q', value: 'hello world' });
    expect(entries[1]).toMatchObject({ key: 'tag', value: 'foo+bar' });
  });

  it('handles duplicate keys as separate entries', () => {
    const entries = parseQueryParams('https://example.com?color=red&color=blue');
    expect(entries).toHaveLength(2);
    expect(entries[0]).toMatchObject({ key: 'color', value: 'red' });
    expect(entries[1]).toMatchObject({ key: 'color', value: 'blue' });
  });

  it('handles empty values', () => {
    const entries = parseQueryParams('https://example.com?empty=&present=1');
    expect(entries[0]).toMatchObject({ key: 'empty', value: '' });
    expect(entries[1]).toMatchObject({ key: 'present', value: '1' });
  });

  it('handles keys with no = sign as empty values', () => {
    const entries = parseQueryParams('https://example.com?flag');
    expect(entries[0]).toMatchObject({ key: 'flag', value: '' });
  });

  it('sets enabled=true on every parsed entry', () => {
    const entries = parseQueryParams('https://example.com?a=1&b=2&c=3');
    expect(entries.every((e) => e.enabled)).toBe(true);
  });
});

// ─── buildUrl ─────────────────────────────────────────────────────────────────

describe('buildUrl', () => {
  it('builds a URL with basic params', () => {
    const result = buildUrl('https://api.example.com/users', [
      { id: '1', key: 'page', value: '1', enabled: true },
      { id: '2', key: 'limit', value: '10', enabled: true },
    ]);
    expect(result).toBe('https://api.example.com/users?page=1&limit=10');
  });

  it('skips disabled params', () => {
    const result = buildUrl('https://api.example.com/users', [
      { id: '1', key: 'page', value: '1', enabled: true },
      { id: '2', key: 'draft', value: 'true', enabled: false },
    ]);
    expect(result).toBe('https://api.example.com/users?page=1');
  });

  it('returns the base URL unchanged when params array is empty', () => {
    expect(buildUrl('https://api.example.com/users', [])).toBe(
      'https://api.example.com/users'
    );
  });

  it('returns the base URL when all params are disabled', () => {
    const result = buildUrl('https://api.example.com/users', [
      { id: '1', key: 'page', value: '1', enabled: false },
    ]);
    expect(result).toBe('https://api.example.com/users');
  });

  it('percent-encodes special characters in keys and values', () => {
    const result = buildUrl('https://example.com/search', [
      { id: '1', key: 'q', value: 'hello world', enabled: true },
      { id: '2', key: 'tag', value: 'foo+bar', enabled: true },
    ]);
    expect(result).toBe('https://example.com/search?q=hello%20world&tag=foo%2Bbar');
  });

  it('preserves empty values in the query string', () => {
    const result = buildUrl('https://example.com', [
      { id: '1', key: 'empty', value: '', enabled: true },
    ]);
    expect(result).toBe('https://example.com?empty=');
  });
});

// ─── extractPathParams ────────────────────────────────────────────────────────

describe('extractPathParams', () => {
  it('extracts colon-style params', () => {
    expect(
      extractPathParams('https://api.example.com/users/:userId')
    ).toEqual(['userId']);
  });

  it('extracts brace-style params', () => {
    expect(
      extractPathParams('https://api.example.com/users/{userId}')
    ).toEqual(['userId']);
  });

  it('extracts multiple mixed-style params', () => {
    const result = extractPathParams(
      'https://api.example.com/users/:userId/posts/{postId}'
    );
    expect(result).toEqual(['userId', 'postId']);
  });

  it('returns an empty array when there are no path params', () => {
    expect(extractPathParams('https://api.example.com/users')).toEqual([]);
  });

  it('does not extract params from the query string', () => {
    // Query string contains :fake and {also_fake} — should be ignored.
    const result = extractPathParams(
      'https://api.example.com/path?q=:fake&other={also_fake}'
    );
    expect(result).toEqual([]);
  });

  it('extracts multiple colon-style params from the same path', () => {
    const result = extractPathParams(
      'https://api.example.com/:org/:repo/issues/:number'
    );
    expect(result).toEqual(['org', 'repo', 'number']);
  });
});

// ─── Roundtrip ────────────────────────────────────────────────────────────────

describe('roundtrip: parse → build → parse', () => {
  it('preserves all params through a full roundtrip', () => {
    const originalUrl = 'https://api.example.com/search?q=hello+world&page=2&sort=desc';
    const entries = parseQueryParams(originalUrl);
    const { base } = splitUrl(originalUrl);
    const rebuilt = buildUrl(base, entries);
    const reparsed = parseQueryParams(rebuilt);

    expect(reparsed).toHaveLength(entries.length);
    reparsed.forEach((entry, i) => {
      expect(entry.key).toBe(entries[i].key);
      expect(entry.value).toBe(entries[i].value);
      expect(entry.enabled).toBe(true);
    });
  });

  it('handles encoded characters consistently across a roundtrip', () => {
    const originalUrl = 'https://example.com?q=hello%20world&tag=a%2Bb';
    const entries = parseQueryParams(originalUrl);
    const { base } = splitUrl(originalUrl);
    const rebuilt = buildUrl(base, entries);
    const reparsed = parseQueryParams(rebuilt);

    expect(reparsed[0]).toMatchObject({ key: 'q', value: 'hello world' });
    expect(reparsed[1]).toMatchObject({ key: 'tag', value: 'a+b' });
  });
});
