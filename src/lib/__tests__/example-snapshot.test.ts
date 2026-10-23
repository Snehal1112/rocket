import { describe, expect, it } from 'vitest';
import type { KeyValueEntry, RequestState, ResponseState } from '@/types/pane-types';
import { buildExampleSnapshot, defaultExampleName } from '../example-snapshot';
import { createDefaultRequest } from '../pane-utils';

function row(key: string, value: string, enabled = true): KeyValueEntry {
  return { id: key, key, value, enabled };
}

function request(extra: Partial<RequestState> = {}): RequestState {
  return { ...createDefaultRequest(), ...extra };
}

function response(extra: Partial<ResponseState> = {}): ResponseState {
  return {
    status: 200,
    statusText: 'OK',
    headers: [row('content-type', 'application/json')],
    body: '{}',
    durationMs: 1,
    ttfbMs: 1,
    sizeBytes: 2,
    activeView: 'pretty',
    ...extra,
  };
}

describe('buildExampleSnapshot', () => {
  it('keeps_templates_and_skips_disabled_rows', () => {
    const snap = buildExampleSnapshot(
      request({
        url: '{{base}}/login',
        headers: [row('X-On', '1'), row('X-Off', '2', false)],
        queryParams: [row('q', '1', false)],
      }),
      response(),
    );
    expect(snap.request.url).toBe('{{base}}/login');
    expect(snap.request.headers).toEqual([{ name: 'X-On', value: '1' }]);
    expect(snap.request.params).toEqual([]);
  });

  it('stores_query_and_path_params', () => {
    const snap = buildExampleSnapshot(
      request({
        url: '/u/:id?a=1',
        queryParams: [row('a', '1')],
        pathParams: [row('id', '7')],
      }),
      response(),
    );
    expect(snap.request.url).toBe('/u/:id?a=1');
    expect(snap.request.params).toEqual([
      { name: 'a', value: '1', type: 'query' },
      { name: 'id', value: '7', type: 'path' },
    ]);
  });

  it('json_body_maps_to_type_and_data', () => {
    const snap = buildExampleSnapshot(
      request({ body: { mode: 'json', content: '{"a":1}', formData: [] } }),
      response(),
    );
    expect(snap.request.body).toEqual({ type: 'json', data: '{"a":1}' });
    expect(snap.notes).toEqual([]);
  });

  it('form_urlencoded_maps_to_a_list', () => {
    const snap = buildExampleSnapshot(
      request({
        body: {
          mode: 'formurlencoded',
          content: '',
          formData: [row('a', '1'), row('b', '2'), row('c', '3', false)],
        },
      }),
      response(),
    );
    expect(snap.request.body).toEqual({
      type: 'form-urlencoded',
      data: [
        { name: 'a', value: '1' },
        { name: 'b', value: '2' },
      ],
    });
  });

  it('formdata_and_binary_bodies_are_dropped_with_a_note', () => {
    for (const mode of ['formdata', 'binary'] as const) {
      const snap = buildExampleSnapshot(
        request({ body: { mode, content: '', formData: [row('a', '1')] } }),
        response(),
      );
      expect(snap.request.body).toBeUndefined();
      expect(snap.notes).toHaveLength(1);
    }
  });

  it('response_body_type_comes_from_content_type', () => {
    const type = (ct: string, extra: Partial<ResponseState> = {}) =>
      buildExampleSnapshot(
        request(),
        response({ headers: [row('Content-Type', ct)], body: 'x', ...extra }),
      ).response.body?.type;
    expect(type('application/problem+json')).toBe('json');
    expect(type('text/html; charset=utf-8')).toBe('html');
    expect(type('application/xml')).toBe('xml');
    expect(type('text/plain')).toBe('text');
    expect(type('application/octet-stream', { isBinary: true })).toBe('binary');
  });

  it('big_response_body_is_dropped_with_a_note', () => {
    const snap = buildExampleSnapshot(request(), response({ body: 'a'.repeat(600000) }));
    expect(snap.response.body).toEqual({ type: 'binary', data: '' });
    expect(snap.notes).toHaveLength(1);
  });

  it('status_and_headers_are_copied', () => {
    const snap = buildExampleSnapshot(request(), response({ status: 201, statusText: 'Created' }));
    expect(snap.response.status).toBe(201);
    expect(snap.response.statusText).toBe('Created');
    expect(snap.response.headers).toEqual([{ name: 'content-type', value: 'application/json' }]);
  });
});

describe('defaultExampleName', () => {
  it('default_name_uses_status_and_text', () => {
    expect(defaultExampleName(response())).toBe('200 OK');
    expect(defaultExampleName(response({ status: 404, statusText: '' }))).toBe('404');
  });
});
