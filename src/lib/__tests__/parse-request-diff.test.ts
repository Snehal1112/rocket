import { describe, it, expect } from 'vitest';
import { parseRequestDiff } from '../parse-request-diff';

describe('parseRequestDiff', () => {
  const baseRequest = {
    method: 'GET',
    url: 'https://api.example.com/users',
    headers: [{ key: 'Accept', value: 'application/json', enabled: true }],
    queryParams: [],
    pathParams: [],
    body: null,
    auth: { type: 'none' },
    preRequestScript: null,
    postResponseScript: null,
  };

  it('returns null when both contents are undefined', () => {
    expect(parseRequestDiff(undefined, undefined)).toBeNull();
  });

  it('returns null when neither is valid JSON', () => {
    expect(parseRequestDiff('not json', 'also not json')).toBeNull();
  });

  it('returns null when JSON lacks method and url', () => {
    expect(parseRequestDiff('{"foo":"bar"}', '{"foo":"baz"}')).toBeNull();
  });

  it('returns a diff when both sides are valid request objects', () => {
    const content = JSON.stringify(baseRequest);
    expect(parseRequestDiff(content, content)).not.toBeNull();
  });

  it('reports no changes when files are identical', () => {
    const content = JSON.stringify(baseRequest);
    const diff = parseRequestDiff(content, content)!;
    expect(diff.hasChanges).toBe(false);
    expect(diff.method.changed).toBe(false);
    expect(diff.url.changed).toBe(false);
    expect(diff.auth.changed).toBe(false);
  });

  it('detects method change', () => {
    const old = JSON.stringify({ ...baseRequest, method: 'GET' });
    const nw = JSON.stringify({ ...baseRequest, method: 'POST' });
    const diff = parseRequestDiff(old, nw)!;
    expect(diff.method.changed).toBe(true);
    expect(diff.method.oldValue).toBe('GET');
    expect(diff.method.newValue).toBe('POST');
    expect(diff.hasChanges).toBe(true);
  });

  it('detects url change', () => {
    const old = JSON.stringify({ ...baseRequest, url: 'https://a.com' });
    const nw = JSON.stringify({ ...baseRequest, url: 'https://b.com' });
    const diff = parseRequestDiff(old, nw)!;
    expect(diff.url.changed).toBe(true);
  });

  it('detects added header', () => {
    const old = JSON.stringify({ ...baseRequest, headers: [] });
    const nw = JSON.stringify({
      ...baseRequest,
      headers: [{ key: 'Authorization', value: 'Bearer token', enabled: true }],
    });
    const diff = parseRequestDiff(old, nw)!;
    const row = diff.headers.find((h) => h.key === 'Authorization')!;
    expect(row.status).toBe('added');
    expect(row.oldRow).toBeUndefined();
    expect(row.newRow?.value).toBe('Bearer token');
  });

  it('detects removed header', () => {
    const old = JSON.stringify({
      ...baseRequest,
      headers: [{ key: 'Accept', value: 'application/json', enabled: true }],
    });
    const nw = JSON.stringify({ ...baseRequest, headers: [] });
    const diff = parseRequestDiff(old, nw)!;
    const row = diff.headers.find((h) => h.key === 'Accept')!;
    expect(row.status).toBe('removed');
    expect(row.newRow).toBeUndefined();
  });

  it('detects modified header value', () => {
    const old = JSON.stringify({
      ...baseRequest,
      headers: [{ key: 'Accept', value: 'application/json', enabled: true }],
    });
    const nw = JSON.stringify({
      ...baseRequest,
      headers: [{ key: 'Accept', value: 'text/plain', enabled: true }],
    });
    const diff = parseRequestDiff(old, nw)!;
    const row = diff.headers.find((h) => h.key === 'Accept')!;
    expect(row.status).toBe('modified');
    expect(row.oldRow?.value).toBe('application/json');
    expect(row.newRow?.value).toBe('text/plain');
  });

  it('handles new file (oldContent undefined)', () => {
    const nw = JSON.stringify(baseRequest);
    const diff = parseRequestDiff(undefined, nw)!;
    expect(diff).not.toBeNull();
    expect(diff.method.oldValue).toBeUndefined();
    expect(diff.method.newValue).toBe('GET');
    expect(diff.method.changed).toBe(true);
  });

  it('handles deleted file (newContent undefined)', () => {
    const old = JSON.stringify(baseRequest);
    const diff = parseRequestDiff(old, undefined)!;
    expect(diff).not.toBeNull();
    expect(diff.method.oldValue).toBe('GET');
    expect(diff.method.newValue).toBeUndefined();
    expect(diff.method.changed).toBe(true);
  });

  it('treats null and undefined script fields as equivalent (no false positive)', () => {
    const a = JSON.stringify({ ...baseRequest, preRequestScript: null });
    const b = JSON.stringify({ ...baseRequest, preRequestScript: undefined });
    const diff = parseRequestDiff(a, b)!;
    expect(diff.preRequestScript.changed).toBe(false);
  });

  it('detects header enabled flag change', () => {
    const old = JSON.stringify({
      ...baseRequest,
      headers: [{ key: 'Accept', value: 'application/json', enabled: true }],
    });
    const nw = JSON.stringify({
      ...baseRequest,
      headers: [{ key: 'Accept', value: 'application/json', enabled: false }],
    });
    const diff = parseRequestDiff(old, nw)!;
    const row = diff.headers.find((h) => h.key === 'Accept')!;
    expect(row.status).toBe('modified');
    expect(row.oldRow?.enabled).toBe(true);
    expect(row.newRow?.enabled).toBe(false);
  });

  it('detects body mode change', () => {
    const old = JSON.stringify({ ...baseRequest, body: null });
    const nw = JSON.stringify({ ...baseRequest, body: { mode: 'json', content: '{}' } });
    const diff = parseRequestDiff(old, nw)!;
    expect(diff.body.changed).toBe(true);
    expect(diff.body.oldValue?.mode).toBe('none');
    expect(diff.body.newValue?.mode).toBe('json');
  });

  it('detects auth type change', () => {
    const old = JSON.stringify({ ...baseRequest, auth: { type: 'none' } });
    const nw = JSON.stringify({ ...baseRequest, auth: { type: 'bearer' } });
    const diff = parseRequestDiff(old, nw)!;
    expect(diff.auth.changed).toBe(true);
    expect(diff.auth.oldValue).toBe('none');
    expect(diff.auth.newValue).toBe('bearer');
  });
});
