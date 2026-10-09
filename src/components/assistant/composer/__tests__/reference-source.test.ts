import { describe, expect, it } from 'vitest';
import type { ReferenceItem } from '@/lib/assistant/types';
import { createDefaultLeaf, createDefaultRequest } from '@/lib/pane-utils';
import type { Folder } from '@/lib/tauri-api';
import type { RequestTab, ResponseState } from '@/types/pane-types';
import {
  environmentReferences,
  filterReferences,
  flattenCollectionTree,
  focusReference,
  lastResponseReference,
} from '../reference-source';

const ROOT: Folder = {
  uid: 'root',
  name: 'shop',
  items: [
    { type: 'summary', uid: 's1', name: 'Health', method: 'GET', url: '/health', fileName: 'health.yml' },
    {
      type: 'folder',
      uid: 'f1',
      name: 'Orders',
      dirName: 'orders',
      items: [
        { type: 'summary', uid: 's2', name: 'List orders', method: 'GET', url: '/orders', fileName: 'list.yml' },
        { type: 'summary', uid: 's3', name: 'Stream', method: 'GET', url: '/ws', fileName: 'stream.yml', kind: 'websocket' },
      ],
    },
  ],
};

const tabWith = (response: ResponseState | null): RequestTab => ({
  id: 't1',
  title: 'List orders',
  isDirty: false,
  tabType: 'request',
  source: { collection: 'shop', path: 'orders/list.yml' },
  request: createDefaultRequest(),
  response,
});

const rootWith = (tab: RequestTab) => ({ ...createDefaultLeaf('g1'), tabs: [tab], activeTabId: tab.id });

const RESPONSE: ResponseState = {
  status: 200,
  statusText: 'OK',
  headers: [],
  body: '{}',
  durationMs: 1,
  ttfbMs: 1,
  sizeBytes: 2,
  activeView: 'pretty',
};

describe('flattenCollectionTree', () => {
  it('lists the collection, folders and HTTP requests with sidebar paths', () => {
    expect(flattenCollectionTree('shop', ROOT)).toEqual<ReferenceItem[]>([
      { kind: 'collection', collection: 'shop', label: 'shop' },
      { kind: 'request', collection: 'shop', path: 'health.yml', label: 'GET Health' },
      { kind: 'folder', collection: 'shop', path: 'orders', label: 'Orders' },
      { kind: 'request', collection: 'shop', path: 'orders/list.yml', label: 'GET List orders' },
    ]);
  });
});

describe('environmentReferences', () => {
  it('uses the environment name as the path', () => {
    expect(environmentReferences('shop', [{ name: 'dev', variables: [] }])).toEqual([
      { kind: 'environment', collection: 'shop', path: 'dev', label: 'env: dev' },
    ]);
  });
});

describe('filterReferences', () => {
  const items = flattenCollectionTree('shop', ROOT);

  it('returns everything up to the limit for an empty query', () => {
    expect(filterReferences(items, '')).toHaveLength(items.length);
    expect(filterReferences(items, '', 2)).toHaveLength(2);
  });

  it('ranks label prefix, then label match, then path match', () => {
    expect(filterReferences(items, 'orders').map((i) => i.label)).toEqual([
      'Orders',
      'GET List orders',
    ]);
    expect(filterReferences(items, 'list.yml').map((i) => i.label)).toEqual(['GET List orders']);
  });
});

describe('focus references', () => {
  it('labels the focus chip with the open tab title', () => {
    const focus = { collection: 'shop', path: 'orders/list.yml' };
    expect(focusReference(focus, rootWith(tabWith(null)))?.label).toBe('List orders');
  });

  it('falls back to the file name without .yml when no tab is open', () => {
    const focus = { collection: 'shop', path: 'orders/list.yml' };
    expect(focusReference(focus, createDefaultLeaf('g1'))?.label).toBe('list');
    expect(focusReference(undefined, createDefaultLeaf('g1'))).toBeNull();
  });

  it('offers the last response only when the focused tab has one', () => {
    const focus = { collection: 'shop', path: 'orders/list.yml' };
    expect(lastResponseReference(focus, rootWith(tabWith(null)))).toBeNull();
    expect(lastResponseReference(focus, rootWith(tabWith(RESPONSE)))).toEqual({
      kind: 'last-response',
      collection: 'shop',
      path: 'orders/list.yml',
      label: 'Last response: List orders',
    });
  });
});
