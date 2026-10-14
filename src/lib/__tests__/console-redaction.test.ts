import { describe, expect, it } from 'vitest';
import {
  authSecretValues,
  buildMaskOptions,
  collectSecretValues,
  hasHiddenValues,
  type HttpExchangeText,
  maskHttpExchange,
  REDACTION_MARKER as M,
  selectHttpView,
} from '../console-redaction';

function exchange(overrides: Partial<HttpExchangeText> = {}): HttpExchangeText {
  return {
    url: 'https://x.test/a',
    requestHeaders: [],
    requestBody: '',
    responseHeaders: [],
    responseBody: '',
    ...overrides,
  };
}

describe('maskHttpExchange', () => {
  it('masks a secret value in the url, headers and bodies of the request and the response', () => {
    const out = maskHttpExchange(
      exchange({
        url: 'https://x.test/a?k=s3cr3t-value',
        requestHeaders: [{ key: 'X-Custom', value: 'a s3cr3t-value b' }],
        requestBody: '{"t":"s3cr3t-value"}',
        responseHeaders: [{ key: 'X-Echo', value: 's3cr3t-value' }],
        responseBody: 's3cr3t-value',
      }),
      { secretValues: ['s3cr3t-value'] },
    );
    expect(out.masked.url).toBe(`https://x.test/a?k=${M}`);
    expect(out.masked.requestHeaders).toEqual([{ key: 'X-Custom', value: `a ${M} b` }]);
    expect(out.masked.requestBody).toBe(`{"t":"${M}"}`);
    expect(out.masked.responseHeaders).toEqual([{ key: 'X-Echo', value: M }]);
    expect(out.masked.responseBody).toBe(M);
    expect(out.changed).toBe(true);
    expect(out.uncertain).toBe(false);
  });

  it('keeps secrets shorter than 6 characters', () => {
    const out = maskHttpExchange(exchange({ requestBody: 'abc12' }), { secretValues: ['abc12'] });
    expect(out.masked.requestBody).toBe('abc12');
    expect(out.changed).toBe(false);
  });

  it('masks a 6 character secret', () => {
    const out = maskHttpExchange(exchange({ requestBody: 'abc123' }), { secretValues: ['abc123'] });
    expect(out.masked.requestBody).toBe(M);
  });

  it('masks the longest secret first', () => {
    const out = maskHttpExchange(exchange({ requestBody: 'abcdefghij' }), {
      secretValues: ['abcdef', 'abcdefghij'],
    });
    expect(out.masked.requestBody).toBe(M);
  });

  it('masks sensitive header values whatever the case and keeps the names', () => {
    const out = maskHttpExchange(
      exchange({
        requestHeaders: [
          { key: 'AUTHORIZATION', value: 'Bearer xyz' },
          { key: 'cookie', value: 'a=b' },
          { key: 'Proxy-Authorization', value: 'Basic q' },
          { key: 'x-api-key', value: 'k' },
          { key: 'Accept', value: 'json' },
        ],
        responseHeaders: [{ key: 'Set-Cookie', value: 's=1' }],
      }),
      { secretValues: [] },
    );
    expect(out.masked.requestHeaders).toEqual([
      { key: 'AUTHORIZATION', value: M },
      { key: 'cookie', value: M },
      { key: 'Proxy-Authorization', value: M },
      { key: 'x-api-key', value: M },
      { key: 'Accept', value: 'json' },
    ]);
    expect(out.masked.responseHeaders).toEqual([{ key: 'Set-Cookie', value: M }]);
  });

  it('keeps an empty sensitive header empty', () => {
    const out = maskHttpExchange(
      exchange({ requestHeaders: [{ key: 'Authorization', value: '' }] }),
      { secretValues: [] },
    );
    expect(out.masked.requestHeaders).toEqual([{ key: 'Authorization', value: '' }]);
  });

  it('masks credentials in the url', () => {
    const out = maskHttpExchange(exchange({ url: 'https://user:pass@host.test/p' }), {
      secretValues: [],
    });
    expect(out.masked.url).toBe(`https://${M}@host.test/p`);
  });

  it('masks the value of the api key query parameter only', () => {
    const out = maskHttpExchange(
      exchange({ url: 'https://h.test/p?api_key=abc&x=1&api_key2=zz#frag' }),
      { secretValues: [], apiKeyQueryParam: 'api_key' },
    );
    expect(out.masked.url).toBe(`https://h.test/p?api_key=${M}&x=1&api_key2=zz#frag`);
  });

  it('leaves an unresolved template untouched', () => {
    const out = maskHttpExchange(exchange({ url: 'https://h.test/{{vault.token}}' }), {
      secretValues: [],
    });
    expect(out.masked.url).toBe('https://h.test/{{vault.token}}');
    expect(out.changed).toBe(false);
  });

  it('passes the uncertain flag through and still masks the sensitive headers', () => {
    const out = maskHttpExchange(
      exchange({ requestHeaders: [{ key: 'Authorization', value: 'Bearer abc' }] }),
      { secretValues: [], uncertain: true },
    );
    expect(out.uncertain).toBe(true);
    expect(out.masked.requestHeaders).toEqual([{ key: 'Authorization', value: M }]);
  });
});

describe('collectSecretValues', () => {
  it('keeps enabled secret values of 6 or more characters, longest first', () => {
    const out = collectSecretValues([
      [
        { value: 'short', enabled: true, secret: true },
        { value: 'abcdef', enabled: true, secret: true },
        { value: 'plain-value', enabled: true, secret: false },
        { value: 'disabled-secret', enabled: false, secret: true },
      ],
      [{ value: '', initialValue: 'fallback-secret', enabled: true, secret: true }],
    ]);
    expect(out).toEqual(['fallback-secret', 'abcdef']);
  });

  it('adds the extra values and drops duplicates', () => {
    const out = collectSecretValues([[{ value: 'abcdef', enabled: true, secret: true }]], [
      'abcdef',
      'tok-123456',
    ]);
    expect(out).toEqual(['tok-123456', 'abcdef']);
  });
});

describe('authSecretValues and buildMaskOptions', () => {
  it('returns the credential of each auth kind', () => {
    expect(authSecretValues({ authType: 'bearer', token: 'tok' })).toEqual(['tok']);
    expect(authSecretValues({ authType: 'basic', username: 'u', password: 'pw' })).toEqual(['pw']);
    expect(
      authSecretValues({ authType: 'api-key', key: 'k', value: 'v', placement: 'header' }),
    ).toEqual(['v']);
    expect(authSecretValues({ authType: 'none' })).toEqual([]);
  });

  it('sets the api key query parameter only for query placement', () => {
    const query = buildMaskOptions(
      { authType: 'api-key', key: 'api_key', value: 'abcdefgh', placement: 'query' },
      [],
      false,
    );
    expect(query.apiKeyQueryParam).toBe('api_key');
    expect(query.secretValues).toContain('abcdefgh');
    const header = buildMaskOptions(
      { authType: 'api-key', key: 'X-K', value: 'abcdefgh', placement: 'header' },
      [],
      true,
    );
    expect(header.apiKeyQueryParam).toBeUndefined();
    expect(header.uncertain).toBe(true);
  });
});

describe('selectHttpView and hasHiddenValues', () => {
  const raw = exchange({ requestBody: 'abcdefgh' });
  const masked = exchange({ requestBody: M });

  it('shows the masked copy unless revealed', () => {
    expect(selectHttpView({ ...raw, masked }, false).requestBody).toBe(M);
    expect(selectHttpView({ ...raw, masked }, true).requestBody).toBe('abcdefgh');
  });

  it('shows the raw fields when there is no masked copy', () => {
    expect(selectHttpView(raw, false).requestBody).toBe('abcdefgh');
  });

  it('never reveals an uncertain row', () => {
    expect(selectHttpView({ ...raw, masked, uncertain: true }, true).requestBody).toBe(M);
  });

  it('reports hidden values only when the masked copy differs', () => {
    expect(hasHiddenValues({ ...raw, masked })).toBe(true);
    expect(hasHiddenValues({ ...raw, masked: raw })).toBe(false);
    expect(hasHiddenValues(raw)).toBe(false);
  });
});
