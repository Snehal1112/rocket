import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  capText,
  chipToResource,
  chipUri,
  RESOURCE_LIMIT_BYTES,
} from '@/lib/assistant/chip-resources';
import type { ReferenceItem } from '@/lib/assistant/types';
import { createDefaultLeaf, createDefaultRequest } from '@/lib/pane-utils';
import {
  type Environment,
  getEnvironment,
  getFolderSettings,
  getRequest,
  type Request,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestTab, ResponseState } from '@/types/pane-types';

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  getRequest: vi.fn(),
  getEnvironment: vi.fn(),
  getFolderSettings: vi.fn(),
  getCollectionSettings: vi.fn(),
}));

const REQUEST_CHIP: ReferenceItem = {
  kind: 'request',
  collection: 'shop',
  path: 'orders/list.yml',
  label: 'GET List orders',
};
const bytes = (text: string) => new TextEncoder().encode(text).length;

function openTab(tab: RequestTab) {
  const leaf = createDefaultLeaf('g1');
  usePaneStore.setState({ root: { ...leaf, tabs: [tab], activeTabId: tab.id }, activeGroupId: 'g1' });
}

function requestTab(overrides: Partial<RequestTab> = {}): RequestTab {
  return {
    id: 't1',
    title: 'List orders',
    isDirty: true,
    tabType: 'request',
    source: { collection: 'shop', path: 'orders/list.yml' },
    request: {
      ...createDefaultRequest(),
      url: 'https://api.test/orders',
      preRequestScript: 'console.log("unsaved edit");',
    },
    response: null,
    ...overrides,
  };
}

beforeEach(() => {
  vi.mocked(getRequest).mockReset();
  vi.mocked(getEnvironment).mockReset();
  vi.mocked(getFolderSettings).mockReset();
  usePaneStore.setState({ root: createDefaultLeaf('g1'), activeGroupId: 'g1' });
});

describe('chipToResource', () => {
  it('uses the open tab, so unsaved script edits are included', async () => {
    openTab(requestTab());
    const resource = await chipToResource(REQUEST_CHIP);
    expect(getRequest).not.toHaveBeenCalled();
    expect(resource.text).toContain('console.log("unsaved edit");');
    expect(resource.text).toContain('unsaved edits');
    expect(resource.uri).toBe('rocket://request/shop/orders/list.yml');
    expect(resource.mimeType).toBe('text/plain');
  });

  it('masks literal credentials of a saved request and keeps variable references', async () => {
    const saved: Request = {
      uid: 'u1',
      name: 'List orders',
      method: 'GET',
      url: 'https://api.test/orders?api_key=live123456',
      headers: [
        { key: 'Authorization', value: 'Bearer abcdefgh12345678', enabled: true },
        { key: 'X-Trace', value: '{{traceId}}', enabled: true },
      ],
      auth: { authType: 'bearer', token: 'abcdefgh12345678' },
      tests: 'test("ok", () => {});',
    };
    vi.mocked(getRequest).mockResolvedValue(saved);
    const resource = await chipToResource(REQUEST_CHIP);
    expect(getRequest).toHaveBeenCalledWith('shop', 'orders/list.yml');
    expect(resource.text).not.toContain('abcdefgh12345678');
    expect(resource.text).not.toContain('live123456');
    expect(resource.text).toContain('<redacted>');
    expect(resource.text).toContain('{{traceId}}');
    expect(resource.text).toContain('Auth: bearer');
    expect(resource.text).toContain('test("ok", () => {});');
  });

  it('shares environment names but not secret values', async () => {
    const env: Environment = {
      name: 'dev',
      variables: [
        { key: 'baseUrl', value: 'https://dev.test', enabled: true, secret: false },
        { key: 'clientSecret', value: 's3cr3t-value', enabled: true, secret: true },
      ],
    };
    vi.mocked(getEnvironment).mockResolvedValue(env);
    const resource = await chipToResource({
      kind: 'environment',
      collection: 'shop',
      path: 'dev',
      label: 'env: dev',
    });
    expect(getEnvironment).toHaveBeenCalledWith('shop', 'dev');
    expect(resource.text).toContain('https://dev.test');
    expect(resource.text).toContain('clientSecret');
    expect(resource.text).not.toContain('s3cr3t-value');
  });

  it('masks response cookies and shows the status and body', async () => {
    const response: ResponseState = {
      status: 200,
      statusText: 'OK',
      headers: [{ id: 'h1', key: 'Set-Cookie', value: 'sid=abc123xyz', enabled: true }],
      body: '{"ok":true}',
      durationMs: 12,
      ttfbMs: 5,
      sizeBytes: 11,
      activeView: 'pretty',
    };
    openTab(requestTab({ response }));
    const resource = await chipToResource({ ...REQUEST_CHIP, kind: 'last-response' });
    expect(resource.text).toContain('Status: 200 OK');
    expect(resource.text).toContain('{"ok":true}');
    expect(resource.text).not.toContain('abc123xyz');
    expect(resource.uri).toBe('rocket://last-response/shop/orders/list.yml');
  });

  it('does not echo a load error', async () => {
    vi.mocked(getFolderSettings).mockRejectedValue(new Error('boom Authorization: Bearer zzzzzzzz9'));
    const resource = await chipToResource({
      kind: 'folder',
      collection: 'shop',
      path: 'orders',
      label: 'orders',
    });
    expect(resource.text).toBe('Rocket could not load this folder: orders.');
  });

  it('caps a large chip at 8 KB with a marker', async () => {
    const tab = requestTab();
    tab.request = { ...tab.request, body: { mode: 'json', content: 'x'.repeat(20_000), formData: [] } };
    openTab(tab);
    const resource = await chipToResource(REQUEST_CHIP);
    expect(bytes(resource.text)).toBeLessThanOrEqual(RESOURCE_LIMIT_BYTES);
    expect(resource.text).toContain('[truncated:');
  });
});

describe('capText', () => {
  it('leaves short text alone', () => {
    expect(capText('hello')).toBe('hello');
  });

  it('never splits a multi-byte character', () => {
    const capped = capText('é'.repeat(5_000));
    expect(bytes(capped)).toBeLessThanOrEqual(RESOURCE_LIMIT_BYTES);
    expect(capped).not.toContain('\uFFFD');
    expect(capped).toContain('[truncated: 10000 bytes cut to 8192]');
  });
});

describe('chipUri', () => {
  it('encodes each segment', () => {
    expect(
      chipUri({ kind: 'request', collection: 'my shop', path: 'a b/list.yml', label: 'x' }),
    ).toBe('rocket://request/my%20shop/a%20b/list.yml');
    expect(chipUri({ kind: 'collection', collection: 'shop', label: 'shop' })).toBe(
      'rocket://collection/shop',
    );
  });
});
