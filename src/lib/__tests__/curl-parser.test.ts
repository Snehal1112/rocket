import { describe, expect, it } from 'vitest';
import { generateCurlCommand } from '@/lib/curl-generator';
import { bodyFromParsedCurl, parseCurl } from '@/lib/curl-parser';
import type { ResolvedRequestFields } from '@/lib/execute-request';

function localResolved(overrides: Partial<ResolvedRequestFields> = {}): ResolvedRequestFields {
  return {
    url: 'https://api.example.com/upload',
    headers: [],
    queryParams: [],
    pathParams: [],
    body: undefined,
    auth: { authType: 'none' },
    assertions: [],
    collection: undefined,
    environmentName: undefined,
    requestPath: undefined,
    ...overrides,
  };
}

describe('parseCurl form data', () => {
  it('parses a text --form field into a form-data body', () => {
    const parsed = parseCurl("curl -F 'name=Ada' https://x.test/up");
    expect(parsed?.url).toBe('https://x.test/up');
    expect(parsed?.method).toBe('POST');
    expect(parsed?.body).toEqual({
      mode: 'formdata',
      content: '',
      formData: [{ key: 'name', value: 'Ada', entryType: 'text', enabled: true }],
    });
  });

  it('parses a file --form field with its type', () => {
    const parsed = parseCurl("curl --form 'file=@/tmp/a.png;type=image/png' https://x.test/up");
    expect(parsed?.body?.formData).toEqual([
      {
        key: 'file',
        value: '/tmp/a.png',
        entryType: 'file',
        enabled: true,
        contentType: 'image/png',
      },
    ]);
  });

  it('drops other ; options from a file field', () => {
    const parsed = parseCurl("curl -F 'f=@a.txt;filename=b.txt' https://x.test");
    const entry = parsed?.body?.formData?.[0];
    expect(entry?.value).toBe('a.txt');
    expect(entry).not.toHaveProperty('contentType');
  });

  it('keeps --form-string values as text, even when they start with @', () => {
    const parsed = parseCurl(
      "curl --form-string 'note=@not-a-file' --form-string 'n2=<x' https://x.test",
    );
    expect(parsed?.body?.formData).toEqual([
      { key: 'note', value: '@not-a-file', entryType: 'text', enabled: true },
      { key: 'n2', value: '<x', entryType: 'text', enabled: true },
    ]);
  });

  it('keeps equals signs inside a value', () => {
    const parsed = parseCurl("curl -F 'token=a=b' https://x.test");
    expect(parsed?.body?.formData).toEqual([
      { key: 'token', value: 'a=b', entryType: 'text', enabled: true },
    ]);
  });

  it('ignores a --form argument without an equals sign', () => {
    const parsed = parseCurl("curl -F 'oops' -F 'a=1' https://x.test");
    expect(parsed?.body?.formData).toEqual([
      { key: 'a', value: '1', entryType: 'text', enabled: true },
    ]);
  });

  it('lets the form body win over -d', () => {
    const parsed = parseCurl("curl -d 'x=1' -F 'a=1' https://x.test");
    expect(parsed?.body?.mode).toBe('formdata');
  });

  it('lets an explicit -X win over the inferred POST', () => {
    const parsed = parseCurl("curl -X PUT -F 'a=1' https://x.test");
    expect(parsed?.method).toBe('PUT');
  });

  it('keeps -d parsing as before', () => {
    const parsed = parseCurl("curl -d '{\"a\":1}' https://x.test");
    expect(parsed?.method).toBe('POST');
    expect(parsed?.body).toEqual({ mode: 'json', content: '{"a":1}' });
  });
});

describe('parseCurl round trip and body mapping', () => {
  const entries = [
    { key: 'name', value: "it's Ada", entryType: 'text' as const, enabled: true },
    { key: 'file', value: 'docs/a b.png', entryType: 'file' as const, enabled: true },
  ];

  it('round-trips a Copy as cURL form-data body', () => {
    const cmd = generateCurlCommand(
      localResolved({ body: { mode: 'formdata', formData: entries } }),
      'POST',
    );
    expect(parseCurl(cmd)?.body?.formData).toEqual(entries);
  });

  it('skips disabled entries when copying, so they do not come back', () => {
    const disabled = { key: 'old', value: 'x', entryType: 'text' as const, enabled: false };
    const cmd = generateCurlCommand(
      localResolved({ body: { mode: 'formdata', formData: [...entries, disabled] } }),
      'POST',
    );
    expect(parseCurl(cmd)?.body?.formData).toEqual(entries);
  });

  it('bodyFromParsedCurl returns undefined without a body', () => {
    expect(bodyFromParsedCurl(undefined)).toBeUndefined();
  });

  it('bodyFromParsedCurl keeps form entries', () => {
    expect(bodyFromParsedCurl(parseCurl("curl -F 'a=1' https://x.test")?.body)).toEqual({
      mode: 'formdata',
      content: '',
      formData: [
        { id: expect.any(String), key: 'a', value: '1', entryType: 'text', enabled: true },
      ],
    });
  });

  it('bodyFromParsedCurl gives each form row its own id', () => {
    const state = bodyFromParsedCurl(parseCurl("curl -F 'a=1' -F 'b=2' https://x.test")?.body);
    const ids = state?.formData.map((row) => row.id) ?? [];
    expect(ids).toHaveLength(2);
    expect(new Set(ids).size).toBe(2);
  });

  it('bodyFromParsedCurl keeps the text body shape', () => {
    expect(bodyFromParsedCurl({ mode: 'json', content: '{"a":1}' })).toEqual({
      mode: 'json',
      content: '{"a":1}',
      formData: [],
    });
  });
});
