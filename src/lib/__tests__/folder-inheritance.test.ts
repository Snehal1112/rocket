import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { FolderSettings } from '@/lib/tauri-api';

const folders = vi.hoisted(() => ({ byPath: new Map<string, unknown>() }));

vi.mock('@/lib/tauri-api', () => ({
  getFolderSettings: vi.fn(async (_collection: string, folderPath: string) => {
    if (!folders.byPath.has(folderPath)) throw new Error(`cannot read ${folderPath}/folder.yml`);
    return folders.byPath.get(folderPath);
  }),
}));

import {
  ancestorFolderPaths,
  inheritedHeaders,
  loadFolderChain,
  resolveFolderAuth,
} from '@/lib/folder-inheritance';
import { getFolderSettings } from '@/lib/tauri-api';

function folder(partial: Partial<FolderSettings>): FolderSettings {
  return { headers: [], variables: [], ...partial } as FolderSettings;
}

const h = (key: string, value: string, enabled = true) => ({ key, value, enabled });

describe('ancestorFolderPaths', () => {
  it('lists every folder above the request, outermost first', () => {
    expect(ancestorFolderPaths('users/admin/get.yml')).toEqual(['users', 'users/admin']);
  });

  it('gives no folders for a request at the collection root', () => {
    expect(ancestorFolderPaths('get.yml')).toEqual([]);
  });

  it('accepts Windows separators', () => {
    expect(ancestorFolderPaths('users\\admin\\get.yml')).toEqual(['users', 'users/admin']);
  });
});

describe('inheritedHeaders', () => {
  it('lets a folder header replace a collection header by key, ignoring case', () => {
    const merged = inheritedHeaders(
      [h('X-Env', 'collection'), h('X-Team', 'core')],
      [folder({ headers: [h('x-env', 'folder')] })],
    );
    expect(merged).toEqual([h('X-Team', 'core'), h('x-env', 'folder')]);
  });

  it('lets the inner folder win over the outer folder', () => {
    const merged = inheritedHeaders(
      [],
      [
        folder({ headers: [h('X-Env', 'outer'), h('X-Outer', 'only')] }),
        folder({ headers: [h('X-Env', 'inner')] }),
      ],
    );
    expect(merged).toEqual([h('X-Outer', 'only'), h('X-Env', 'inner')]);
  });

  it('never lets a disabled header shadow or be sent', () => {
    const merged = inheritedHeaders(
      [h('X-Env', 'collection'), h('X-Off', 'collection', false)],
      [folder({ headers: [h('X-Env', 'folder-off', false)] })],
    );
    expect(merged).toEqual([h('X-Env', 'collection')]);
  });
});

describe('resolveFolderAuth', () => {
  it('returns the innermost folder auth that is not none or inherit', () => {
    const auth = resolveFolderAuth([
      folder({ auth: { authType: 'bearer', token: 'outer' } }),
      folder({ auth: { authType: 'bearer', token: 'inner' } }),
      folder({ auth: { authType: 'inherit' } }),
      folder({ auth: { authType: 'none' } }),
    ]);
    expect(auth).toEqual({ authType: 'bearer', token: 'inner' });
  });

  it('returns undefined when no folder sets auth', () => {
    expect(resolveFolderAuth([folder({}), folder({ auth: { authType: 'inherit' } })])).toBe(
      undefined,
    );
  });
});

describe('loadFolderChain', () => {
  beforeEach(() => {
    folders.byPath.clear();
    vi.clearAllMocks();
  });

  it('reads each ancestor folder outermost first and skips one that cannot be read', async () => {
    const outer = folder({ headers: [h('X-A', '1')] });
    folders.byPath.set('users', outer);

    const chain = await loadFolderChain('api', 'users/admin/get.yml');

    expect(chain).toEqual([outer]);
    expect(getFolderSettings).toHaveBeenCalledTimes(2);
    expect(getFolderSettings).toHaveBeenNthCalledWith(1, 'api', 'users');
    expect(getFolderSettings).toHaveBeenNthCalledWith(2, 'api', 'users/admin');
  });

  it('reads nothing for a request at the collection root', async () => {
    expect(await loadFolderChain('api', 'get.yml')).toEqual([]);
    expect(getFolderSettings).not.toHaveBeenCalled();
  });
});
