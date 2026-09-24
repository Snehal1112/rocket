import { execSync } from 'node:child_process';
import { describe, expect, it } from 'vitest';
import { generateCurlCommand } from '@/lib/curl-generator';
import type { ResolvedRequestFields } from '@/lib/execute-request';

function baseResolved(overrides: Partial<ResolvedRequestFields> = {}): ResolvedRequestFields {
  return {
    url: 'https://api.example.com/users',
    headers: [],
    queryParams: [],
    body: undefined,
    auth: { authType: 'none' },
    assertions: [],
    collection: undefined,
    environmentName: undefined,
    requestPath: undefined,
    ...overrides,
  };
}

describe('generateCurlCommand', () => {
  it('leaves a url with no shell-special characters bare (unquoted)', () => {
    const cmd = generateCurlCommand(baseResolved(), 'GET');
    expect(cmd).toBe('curl -X GET https://api.example.com/users');
  });

  it('quotes the url when it contains a query string', () => {
    const cmd = generateCurlCommand(
      baseResolved({ url: 'https://api.example.com/users?sort=asc' }),
      'GET',
    );
    expect(cmd).toContain("curl -X GET 'https://api.example.com/users?sort=asc'");
  });

  describe('headers', () => {
    it('adds one -H flag per enabled header, always quoted (space between key and value)', () => {
      const cmd = generateCurlCommand(
        baseResolved({
          headers: [
            { key: 'Accept', value: 'application/json', enabled: true },
            { key: 'X-Custom', value: 'value1', enabled: true },
          ],
        }),
        'GET',
      );
      expect(cmd).toContain("-H 'Accept: application/json'");
      expect(cmd).toContain("-H 'X-Custom: value1'");
    });

    it('skips disabled headers', () => {
      const cmd = generateCurlCommand(
        baseResolved({
          headers: [{ key: 'X-Skip', value: 'nope', enabled: false }],
        }),
        'GET',
      );
      expect(cmd).not.toContain('X-Skip');
    });
  });

  describe('shell escaping', () => {
    it('leaves a value with only safe characters bare (no quotes)', () => {
      const cmd = generateCurlCommand(
        baseResolved({
          body: {
            mode: 'formurlencoded',
            formData: [{ key: 'id', value: '42', entryType: 'text', enabled: true }],
          },
        }),
        'POST',
      );
      expect(cmd).toContain('--data-urlencode id=42');
      expect(cmd).not.toContain("'id=42'");
    });

    it('single-quotes a value containing spaces', () => {
      const cmd = generateCurlCommand(
        baseResolved({ headers: [{ key: 'X-Note', value: 'hello world', enabled: true }] }),
        'GET',
      );
      expect(cmd).toContain("-H 'X-Note: hello world'");
    });

    it("escapes an embedded single quote using the '\\'' idiom", () => {
      const cmd = generateCurlCommand(
        baseResolved({ headers: [{ key: 'X-Quote', value: "it's here", enabled: true }] }),
        'GET',
      );
      expect(cmd).toContain("-H 'X-Quote: it'\\''s here'");
    });
  });

  describe('auth', () => {
    it('emits nothing extra for authType none', () => {
      const cmd = generateCurlCommand(baseResolved({ auth: { authType: 'none' } }), 'GET');
      expect(cmd).not.toContain('-u');
      expect(cmd).not.toContain('Authorization');
    });

    it('emits -u user:pass (bare, colon is shell-safe) for basic auth', () => {
      const cmd = generateCurlCommand(
        baseResolved({ auth: { authType: 'basic', username: 'alice', password: 'secret' } }),
        'GET',
      );
      expect(cmd).toContain('-u alice:secret');
    });

    it('quotes -u when credentials contain a space', () => {
      const cmd = generateCurlCommand(
        baseResolved({ auth: { authType: 'basic', username: 'alice smith', password: 'secret' } }),
        'GET',
      );
      expect(cmd).toContain("-u 'alice smith:secret'");
    });

    it('emits an Authorization: Bearer header for bearer auth', () => {
      const cmd = generateCurlCommand(
        baseResolved({ auth: { authType: 'bearer', token: 'abc.def.ghi' } }),
        'GET',
      );
      expect(cmd).toContain("-H 'Authorization: Bearer abc.def.ghi'");
    });

    it('emits a named header for api-key auth placed in header', () => {
      const cmd = generateCurlCommand(
        baseResolved({
          auth: { authType: 'api-key', key: 'X-API-Key', value: 'k-123', placement: 'header' },
        }),
        'GET',
      );
      expect(cmd).toContain("-H 'X-API-Key: k-123'");
    });

    it('appends to the query string for api-key auth placed in query', () => {
      const cmd = generateCurlCommand(
        baseResolved({
          auth: { authType: 'api-key', key: 'apiKey', value: 'k-123', placement: 'query' },
        }),
        'GET',
      );
      expect(cmd).toContain('apiKey=k-123');
      expect(cmd).not.toContain('-H');
    });
  });

  describe('body modes', () => {
    it('adds --data plus an auto Content-Type header for json body', () => {
      const cmd = generateCurlCommand(
        baseResolved({ body: { mode: 'json', content: '{"a": 1}' } }),
        'POST',
      );
      expect(cmd).toContain('--data \'{"a": 1}\'');
      expect(cmd).toContain("-H 'Content-Type: application/json'");
    });

    it('does not duplicate Content-Type if the request already sets one', () => {
      const cmd = generateCurlCommand(
        baseResolved({
          headers: [
            { key: 'Content-Type', value: 'application/json; charset=utf-8', enabled: true },
          ],
          body: { mode: 'json', content: '{"a": 1}' },
        }),
        'POST',
      );
      const matches = cmd.match(/Content-Type/g) ?? [];
      expect(matches).toHaveLength(1);
    });

    it('adds --data plus text/plain Content-Type for text body', () => {
      const cmd = generateCurlCommand(
        baseResolved({ body: { mode: 'text', content: 'hello world' } }),
        'POST',
      );
      expect(cmd).toContain("--data 'hello world'");
      expect(cmd).toContain("-H 'Content-Type: text/plain'");
    });

    it('adds --data plus application/xml Content-Type for xml body', () => {
      const cmd = generateCurlCommand(
        baseResolved({ body: { mode: 'xml', content: '<a>1</a>' } }),
        'POST',
      );
      expect(cmd).toContain("--data '<a>1</a>'");
      expect(cmd).toContain("-H 'Content-Type: application/xml'");
    });

    it('emits no --data flag when raw body content is empty', () => {
      const cmd = generateCurlCommand(
        baseResolved({ body: { mode: 'json', content: '' } }),
        'POST',
      );
      expect(cmd).not.toContain('--data');
      expect(cmd).not.toContain('Content-Type');
    });

    it('emits one --data-urlencode per enabled field for formurlencoded body', () => {
      const cmd = generateCurlCommand(
        baseResolved({
          body: {
            mode: 'formurlencoded',
            formData: [
              { key: 'name', value: 'John Doe', entryType: 'text', enabled: true },
              { key: 'skip', value: 'me', entryType: 'text', enabled: false },
            ],
          },
        }),
        'POST',
      );
      expect(cmd).toContain("--data-urlencode 'name=John Doe'");
      expect(cmd).not.toContain('skip=me');
    });

    it('emits --form for text fields and --form name=@path for file fields', () => {
      const cmd = generateCurlCommand(
        baseResolved({
          body: {
            mode: 'formdata',
            formData: [
              { key: 'title', value: 'my file', entryType: 'text', enabled: true },
              { key: 'avatar', value: '/tmp/a b.png', entryType: 'file', enabled: true },
            ],
          },
        }),
        'POST',
      );
      expect(cmd).toContain("--form 'title=my file'");
      expect(cmd).toContain("--form 'avatar=@/tmp/a b.png'");
    });

    it('emits --data-binary @path for binary body, bare when the path has no special characters', () => {
      const cmd = generateCurlCommand(
        baseResolved({ body: { mode: 'binary', content: '/tmp/payload.bin' } }),
        'POST',
      );
      expect(cmd).toContain('--data-binary @/tmp/payload.bin');
    });

    it('emits no body flags for mode none', () => {
      const cmd = generateCurlCommand(baseResolved({ body: undefined }), 'GET');
      expect(cmd).not.toContain('--data');
      expect(cmd).not.toContain('--form');
    });
  });

  describe('query params', () => {
    it('appends enabled query params to a url with no existing query string', () => {
      const cmd = generateCurlCommand(
        baseResolved({
          queryParams: [{ key: 'page', value: '2', enabled: true }],
        }),
        'GET',
      );
      expect(cmd).toContain('https://api.example.com/users?page=2');
    });

    it('merges resolved query params with a query string already in the url', () => {
      const cmd = generateCurlCommand(
        baseResolved({
          url: 'https://api.example.com/users?sort=asc',
          queryParams: [{ key: 'page', value: '2', enabled: true }],
        }),
        'GET',
      );
      expect(cmd).toContain('sort=asc');
      expect(cmd).toContain('page=2');
    });

    it('skips disabled query params', () => {
      const cmd = generateCurlCommand(
        baseResolved({ queryParams: [{ key: 'debug', value: '1', enabled: false }] }),
        'GET',
      );
      expect(cmd).not.toContain('debug=1');
    });
  });

  describe('must run when pasted (POSIX shell syntax check)', () => {
    const cases: { name: string; resolved: ResolvedRequestFields; method: string }[] = [
      { name: 'simple GET', resolved: baseResolved(), method: 'GET' },
      {
        name: 'POST with json body and bearer auth',
        resolved: baseResolved({
          auth: { authType: 'bearer', token: "weird'token" },
          headers: [{ key: 'X-Note', value: "it's a test", enabled: true }],
          body: { mode: 'json', content: '{"a":1,"b":"with \'quote\'"}' },
        }),
        method: 'POST',
      },
      {
        name: 'multipart form with file field',
        resolved: baseResolved({
          body: {
            mode: 'formdata',
            formData: [
              { key: 'title', value: "O'Brien's file", entryType: 'text', enabled: true },
              { key: 'avatar', value: '/tmp/a b.png', entryType: 'file', enabled: true },
            ],
          },
        }),
        method: 'POST',
      },
      {
        name: 'basic auth with special characters',
        resolved: baseResolved({
          auth: { authType: 'basic', username: "us'er", password: 'p@ss w0rd!' },
        }),
        method: 'GET',
      },
    ];

    it.each(cases)('produces syntactically valid bash for: $name', ({ resolved, method }) => {
      const cmd = generateCurlCommand(resolved, method);
      expect(() => execSync('bash -n', { input: cmd })).not.toThrow();
    });
  });
});
