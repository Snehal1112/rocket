import { describe, expect, it } from 'vitest';
import { flattenRunnerEntries } from '@/lib/runner-flatten';
import type { Collection } from '@/lib/tauri-api';

function makeCollection(): Collection {
  return {
    name: 'demo',
    settings: { headers: [], variables: [] } as unknown as Collection['settings'],
    root: {
      uid: 'root',
      name: 'demo',
      items: [
        {
          type: 'request',
          uid: 'r1',
          name: 'Root Request',
          method: 'GET',
          url: 'https://example.com/root',
          headers: [],
          auth: { authType: 'none' },
          fileName: 'root-request.yml',
        },
        {
          type: 'folder',
          uid: 'f1',
          name: 'Auth',
          dirName: 'auth',
          items: [
            {
              type: 'request',
              uid: 'r2',
              name: 'Login',
              method: 'POST',
              url: 'https://example.com/login',
              headers: [],
              auth: { authType: 'none' },
              fileName: 'login.yml',
            },
            {
              type: 'folder',
              uid: 'f2',
              name: 'Nested',
              dirName: 'nested',
              items: [
                {
                  type: 'request',
                  uid: 'r3',
                  name: 'Refresh',
                  method: 'POST',
                  url: 'https://example.com/refresh',
                  headers: [],
                  auth: { authType: 'none' },
                  fileName: 'refresh.yml',
                },
              ],
            },
          ],
        },
      ],
    },
  };
}

describe('flattenRunnerEntries', () => {
  it('flattens the whole collection in tree order, folders first', () => {
    const entries = flattenRunnerEntries(makeCollection());
    expect(entries.map((e) => e.requestPath)).toEqual([
      'auth/login.yml',
      'auth/nested/refresh.yml',
      'root-request.yml',
    ]);
  });

  it('defaults every entry to included and pending', () => {
    const entries = flattenRunnerEntries(makeCollection());
    for (const e of entries) {
      expect(e.included).toBe(true);
      expect(e.status).toBe('pending');
    }
  });

  it('scopes to a folder when folderPath is given', () => {
    const entries = flattenRunnerEntries(makeCollection(), 'auth');
    expect(entries.map((e) => e.requestPath)).toEqual([
      'auth/login.yml',
      'auth/nested/refresh.yml',
    ]);
  });

  it('scopes to a nested folder', () => {
    const entries = flattenRunnerEntries(makeCollection(), 'auth/nested');
    expect(entries.map((e) => e.requestPath)).toEqual(['auth/nested/refresh.yml']);
  });

  it('returns an empty array for a folder with no requests', () => {
    const collection = makeCollection();
    collection.root.items = [
      { type: 'folder', uid: 'f1', name: 'Empty', dirName: 'empty', items: [] },
    ];
    expect(flattenRunnerEntries(collection)).toEqual([]);
  });

  it('returns an empty array when folderPath does not exist', () => {
    expect(flattenRunnerEntries(makeCollection(), 'does-not-exist')).toEqual([]);
  });

  it('carries the full request object onto each entry', () => {
    const entries = flattenRunnerEntries(makeCollection());
    expect(entries[0].request).toMatchObject({ name: 'Login', method: 'POST', uid: 'r2' });
  });
});
