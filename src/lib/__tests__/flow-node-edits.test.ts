import { describe, expect, it } from 'vitest';
import {
  indexWiresOutOfRange,
  inlineHasContent,
  requestEntriesOf,
  savedToInline,
} from '@/lib/flow-node-edits';
import type { FlowEdge, Folder, Request } from '@/lib/tauri-api';

const saved: Request = {
  uid: 'u1',
  name: 'Login',
  method: 'POST',
  url: '{{baseUrl}}/login',
  headers: [
    { key: 'Content-Type', value: 'application/json', enabled: true },
    { key: 'X-Debug', value: '1', enabled: false },
  ],
  body: { mode: 'json', content: '{"user":"{{user}}"}' },
  auth: { authType: 'inherit' },
};

describe('savedToInline', () => {
  it('copies method, url, enabled headers and a raw body', () => {
    const { inline, dropped } = savedToInline(saved);
    expect(inline).toEqual({
      method: 'POST',
      url: '{{baseUrl}}/login',
      headers: [{ name: 'Content-Type', value: 'application/json' }],
      body: '{"user":"{{user}}"}',
    });
    expect(dropped).toEqual(['1 disabled header']);
  });

  it('drops a form body and names it', () => {
    const { inline, dropped } = savedToInline({
      ...saved,
      headers: [],
      body: {
        mode: 'formdata',
        formData: [{ key: 'a', value: 'b', entryType: 'text', enabled: true }],
      },
    });
    expect(inline.body).toBeNull();
    expect(dropped).toEqual(['formdata body']);
  });

  it('treats a none or empty raw body as no body without dropping anything', () => {
    expect(savedToInline({ ...saved, headers: [], body: { mode: 'none' } })).toEqual({
      inline: { method: 'POST', url: '{{baseUrl}}/login', headers: [], body: null },
      dropped: [],
    });
    expect(
      savedToInline({ ...saved, headers: [], body: { mode: 'text', content: '' } }).inline.body,
    ).toBeNull();
    expect(savedToInline({ ...saved, headers: [], body: undefined }).inline.body).toBeNull();
  });

  it('names auth, scripts, tests and assertions it cannot carry', () => {
    const { dropped } = savedToInline({
      ...saved,
      headers: [],
      body: undefined,
      auth: { authType: 'bearer', token: 't' },
      preRequestScript: 'rok.setVar("a", 1);',
      postResponseScript: '  ',
      tests: 'test("ok", () => {});',
      assertions: [{ expression: 'res.status', operator: 'eq', value: '200' }],
    });
    expect(dropped).toEqual(['bearer auth', 'pre-request script', 'tests', 'assertions']);
  });

  it('names enabled actions and ignores disabled ones', () => {
    const action = {
      phase: 'after-response' as const,
      selector: { expression: 'body.id', method: 'jsonq' as const },
      variable: { name: 'id', scope: 'runtime' as const },
    };
    const base = { ...saved, headers: [], body: undefined };
    expect(savedToInline({ ...base, actions: [action] }).dropped).toEqual(['actions']);
    expect(savedToInline({ ...base, actions: [{ ...action, disabled: true }] }).dropped).toEqual(
      [],
    );
  });

  it('keeps nothing for none or inherit auth', () => {
    expect(savedToInline({ ...saved, headers: [], auth: { authType: 'none' } }).dropped).toEqual(
      [],
    );
  });
});

describe('inlineHasContent', () => {
  const empty = { method: 'GET', url: '', headers: [], body: null };

  it('is false for an empty inline request', () => {
    expect(inlineHasContent(empty)).toBe(false);
    expect(inlineHasContent({ ...empty, url: '   ', body: '  ' })).toBe(false);
  });

  it('is true when a url, a header or a body is set', () => {
    expect(inlineHasContent({ ...empty, url: 'https://x' })).toBe(true);
    expect(inlineHasContent({ ...empty, headers: [{ name: '', value: '' }] })).toBe(true);
    expect(inlineHasContent({ ...empty, body: '{}' })).toBe(true);
  });
});

describe('indexWiresOutOfRange', () => {
  const wire = (id: string, targetNodeId: string, targetField: string): FlowEdge => ({
    id,
    sourceNodeId: 'src',
    targetNodeId,
    targetField,
    expression: 'response.body',
  });
  const edges = [
    wire('e0', 'n1', 'headers[0].value'),
    wire('e2', 'n1', 'headers[2].value'),
    wire('eName', 'n1', 'headers[Authorization].value'),
    wire('eUrl', 'n1', 'url'),
    wire('eOther', 'n2', 'headers[5].value'),
  ];

  it('returns only index wires into the node at or past the header count', () => {
    expect(indexWiresOutOfRange(edges, 'n1', 2).map((e) => e.id)).toEqual(['e2']);
    expect(indexWiresOutOfRange(edges, 'n1', 3)).toEqual([]);
    expect(indexWiresOutOfRange(edges, 'n1', 0).map((e) => e.id)).toEqual(['e0', 'e2']);
  });
});

describe('requestEntriesOf', () => {
  const root: Folder = {
    uid: 'root',
    name: 'demo',
    items: [
      { type: 'request', ...saved, fileName: 'login.yml' },
      {
        type: 'folder',
        uid: 'f1',
        name: 'Auth',
        dirName: 'auth',
        items: [
          {
            type: 'summary',
            uid: 's1',
            name: 'Refresh',
            method: 'POST',
            url: '/r',
            fileName: 'refresh.yml',
          },
          {
            type: 'folder',
            uid: 'f2',
            name: 'Admin',
            items: [{ type: 'summary', uid: 's2', name: 'users', method: 'GET', url: '/u' }],
          },
          { type: 'opaque', protocol: 'graphql', name: 'gql', raw: {} },
        ],
      },
    ],
  };

  it('lists every request with the same path the sidebar uses', () => {
    expect(requestEntriesOf(root)).toEqual([
      { path: 'login.yml', name: 'Login', method: 'POST' },
      { path: 'auth/refresh.yml', name: 'Refresh', method: 'POST' },
      { path: 'auth/Admin/users', name: 'users', method: 'GET' },
    ]);
  });
});
