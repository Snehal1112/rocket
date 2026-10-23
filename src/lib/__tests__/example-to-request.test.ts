import { describe, expect, it } from 'vitest';
import type { RequestExample } from '@/lib/tauri-api';
import type { RequestState } from '@/types/pane-types';
import { exampleToRequestState } from '../example-to-request';
import { createDefaultRequest } from '../pane-utils';

function base(extra: Partial<RequestState> = {}): RequestState {
  return {
    ...createDefaultRequest(),
    method: 'GET',
    url: 'https://base.test/old',
    auth: { authType: 'bearer', bearer: { token: '{{parent_token}}' } } as RequestState['auth'],
    preRequestScript: 'rok.log("pre");',
    tags: ['parent'],
    ...extra,
  };
}

describe('exampleToRequestState', () => {
  it('replaces_request_parts_and_keeps_the_rest', () => {
    const example: RequestExample = {
      name: '200 OK',
      request: {
        method: 'POST',
        url: 'https://api.test/login/:id?a=1',
        headers: [{ name: 'X-Id', value: '7' }],
        params: [{ name: 'id', value: '42', type: 'path' }],
        body: { type: 'json', data: '{"a":1}' },
      },
    };
    const result = exampleToRequestState(example, base());
    expect(result.method).toBe('POST');
    expect(result.url).toBe('https://api.test/login/:id?a=1');
    expect(result.headers.map((h) => [h.key, h.value, h.enabled])).toEqual([['X-Id', '7', true]]);
    expect(result.queryParams.map((q) => [q.key, q.value])).toEqual([['a', '1']]);
    expect(result.pathParams.map((p) => [p.key, p.value])).toEqual([['id', '42']]);
    expect(result.body).toMatchObject({ mode: 'json', content: '{"a":1}' });
    expect(result.auth).toEqual(base().auth);
    expect(result.preRequestScript).toBe('rok.log("pre");');
    expect(result.tags).toEqual(['parent']);
    expect(result.settings).toEqual(base().settings);
  });

  it('form_urlencoded_data_becomes_rows_and_text_bodies_become_content', () => {
    const form = exampleToRequestState(
      {
        name: 'f',
        request: { body: { type: 'form-urlencoded', data: [{ name: 'u', value: 'x' }] } },
      },
      base(),
    );
    expect(form.body.mode).toBe('formurlencoded');
    expect(form.body.formData.map((r) => [r.key, r.value, r.enabled])).toEqual([['u', 'x', true]]);

    const text = exampleToRequestState(
      { name: 't', request: { body: { type: 'text', data: 'hello' } } },
      base(),
    );
    expect(text.body).toMatchObject({ mode: 'text', content: 'hello' });
  });

  it('missing_parts_keep_base_values', () => {
    const parent = base({ headers: [{ id: 'h', key: 'K', value: 'V', enabled: true }] });
    const result = exampleToRequestState({ name: 'empty' }, parent);
    expect(result.method).toBe('GET');
    expect(result.url).toBe('https://base.test/old');
    expect(result.headers).toEqual(parent.headers);
    expect(result.body).toEqual(parent.body);
  });

  it('templates_are_kept', () => {
    const result = exampleToRequestState(
      {
        name: 't',
        request: {
          url: '{{base}}/users/:id',
          headers: [{ name: 'Authorization', value: 'Bearer {{token}}' }],
        },
      },
      base(),
    );
    expect(result.url).toBe('{{base}}/users/:id');
    expect(result.headers[0].value).toBe('Bearer {{token}}');
  });
});
