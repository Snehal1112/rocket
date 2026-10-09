import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  capText,
  chipToResource,
  chipUri,
  isChipLoadFailure,
  RESOURCE_LIMIT_BYTES,
} from '@/lib/assistant/chip-resources';
import type { ReferenceItem } from '@/lib/assistant/types';
import { createDefaultLeaf, createDefaultRequest } from '@/lib/pane-utils';
import { buildAssistantChipResource, maskAssistantResponse } from '@/lib/tauri-api';
import { useEnvStore } from '@/stores/env-store';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestTab, ResponseState } from '@/types/pane-types';

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  buildAssistantChipResource: vi.fn(),
  maskAssistantResponse: vi.fn(),
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

const RESPONSE: ResponseState = {
  status: 200,
  statusText: 'OK',
  headers: [{ id: 'h1', key: 'Set-Cookie', value: 'sid=abc123xyz', enabled: true }],
  body: '{"ok":true}',
  durationMs: 12,
  ttfbMs: 5,
  sizeBytes: 11,
  activeView: 'pretty',
  testResults: [
    { name: 'ok', status: 'passed', error: null },
    { name: 'bad', status: 'failed', error: 'boom' },
  ],
};

function requestTab(response: ResponseState | null): RequestTab {
  return {
    id: 't1',
    title: 'List orders',
    isDirty: false,
    tabType: 'request',
    source: { collection: 'shop', path: 'orders/list.yml' },
    request: { ...createDefaultRequest(), url: 'https://api.test/orders' },
    response,
  };
}

beforeEach(() => {
  vi.mocked(buildAssistantChipResource).mockReset();
  vi.mocked(maskAssistantResponse).mockReset();
  usePaneStore.setState({ root: createDefaultLeaf('g1'), activeGroupId: 'g1' });
  useEnvStore.setState({ activeEnvId: null });
});

describe('chipToResource', () => {
  it('takes the text of a saved chip from the backend, as it is', async () => {
    vi.mocked(buildAssistantChipResource).mockResolvedValue({
      uri: 'rocket://request/shop/orders/list.yml',
      mimeType: 'text/plain',
      text: 'masked by the backend',
    });
    const resource = await chipToResource(REQUEST_CHIP);
    expect(buildAssistantChipResource).toHaveBeenCalledWith(
      'request',
      'shop',
      'orders/list.yml',
    );
    expect(resource.text).toBe('masked by the backend');
    expect(resource.uri).toBe('rocket://request/shop/orders/list.yml');
  });

  it('sends a collection chip without a path', async () => {
    vi.mocked(buildAssistantChipResource).mockResolvedValue({
      uri: 'rocket://collection/shop',
      mimeType: 'text/plain',
      text: 'x',
    });
    await chipToResource({ kind: 'collection', collection: 'shop', label: 'shop' });
    expect(buildAssistantChipResource).toHaveBeenCalledWith('collection', 'shop', undefined);
  });

  it('sends the open tab response and request to the backend for masking', async () => {
    const tab = requestTab(RESPONSE);
    tab.request = {
      ...tab.request,
      headers: [{ id: 'k1', key: 'Authorization', value: 'Bearer unsaved-token', enabled: true }],
    };
    openTab(tab);
    useEnvStore.setState({ activeEnvId: 'dev' });
    vi.mocked(maskAssistantResponse).mockResolvedValue({
      uri: 'rocket://last-response/shop/orders/list.yml',
      mimeType: 'text/plain',
      text: 'masked response',
    });
    const resource = await chipToResource({ ...REQUEST_CHIP, kind: 'last-response' });
    expect(buildAssistantChipResource).not.toHaveBeenCalled();
    expect(maskAssistantResponse).toHaveBeenCalledWith(
      'shop',
      'orders/list.yml',
      expect.objectContaining({
        method: 'GET',
        url: 'https://api.test/orders',
        status: 200,
        statusText: 'OK',
        durationMs: 12,
        sizeBytes: 11,
        headers: [{ key: 'Set-Cookie', value: 'sid=abc123xyz' }],
        body: '{"ok":true}',
        isBinary: false,
        tests: [
          { name: 'ok', passed: true, error: null },
          { name: 'bad', passed: false, error: 'boom' },
        ],
        request: expect.objectContaining({
          headers: [{ key: 'Authorization', value: 'Bearer unsaved-token', enabled: true }],
          queryParams: [],
        }),
      }),
      'dev',
    );
    expect(resource.text).toBe('masked response');
  });

  it('says so when the tab has no response, without calling the backend', async () => {
    openTab(requestTab(null));
    const resource = await chipToResource({ ...REQUEST_CHIP, kind: 'last-response' });
    expect(maskAssistantResponse).not.toHaveBeenCalled();
    expect(resource.text).toBe('No response is available for GET List orders.');
  });

  it('does not echo a load error', async () => {
    vi.mocked(buildAssistantChipResource).mockRejectedValue(
      new Error('boom Authorization: Bearer zzzzzzzz9'),
    );
    const resource = await chipToResource({
      kind: 'folder',
      collection: 'shop',
      path: 'orders',
      label: 'orders',
    });
    expect(resource.text).toBe('Rocket could not load this folder: orders.');
    expect(resource.uri).toBe('rocket://folder/shop/orders');
  });

  it('caps a large backend text at 8 KB with a marker', async () => {
    vi.mocked(buildAssistantChipResource).mockResolvedValue({
      uri: 'rocket://request/shop/orders/list.yml',
      mimeType: 'text/plain',
      text: 'x'.repeat(20_000),
    });
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

describe('isChipLoadFailure', () => {
  it('tells the load-failure placeholder from a real resource', () => {
    expect(
      isChipLoadFailure({ uri: 'u', mimeType: 'text/plain', text: 'Rocket could not load this request: x.' }),
    ).toBe(true);
    expect(isChipLoadFailure({ uri: 'u', mimeType: 'text/plain', text: 'Request: x' })).toBe(false);
  });
});
