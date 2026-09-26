import { describe, expect, it } from 'vitest';
import type { CollectionItem } from '@/lib/tauri-api';
import { collectPaths } from './collectPaths';

describe('collectPaths', () => {
  it('skips opaque protocol items so they are never treated as request paths', () => {
    const items: CollectionItem[] = [
      {
        type: 'summary',
        uid: 'r1',
        name: 'Get Users',
        method: 'GET',
        url: '/users',
        fileName: 'get-users.yml',
      },
      { type: 'opaque', protocol: 'graphql', name: 'List Users', raw: {} },
      {
        type: 'folder',
        uid: 'f1',
        name: 'auth',
        dirName: 'auth',
        items: [{ type: 'opaque', protocol: 'websocket', name: 'Chat', raw: {} }],
      },
    ];
    const folders: string[] = [];
    const requests: string[] = [];
    collectPaths(items, '', folders, requests);
    expect(folders).toEqual(['auth']);
    expect(requests).toEqual(['get-users.yml']);
  });
});
