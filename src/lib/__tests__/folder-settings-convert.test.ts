import { describe, expect, it } from 'vitest';
import { entriesToHeaders, folderChainPath, headersToEntries } from '@/lib/folder-settings-convert';

describe('headersToEntries', () => {
  it('maps headers to editor rows with unique ids', () => {
    const rows = headersToEntries([
      { key: 'X-A', value: '1', enabled: true },
      { key: 'X-B', value: '2', enabled: false },
    ]);
    expect(rows.map((r) => [r.key, r.value, r.enabled])).toEqual([
      ['X-A', '1', true],
      ['X-B', '2', false],
    ]);
    expect(new Set(rows.map((r) => r.id)).size).toBe(2);
  });
});

describe('entriesToHeaders', () => {
  it('drops blank-key rows and keeps enabled false', () => {
    const out = entriesToHeaders(
      [
        { id: '0', key: 'X-A', value: '1', enabled: false },
        { id: '1', key: '', value: 'draft', enabled: true },
      ],
      [],
    );
    expect(out).toEqual([{ key: 'X-A', value: '1', enabled: false }]);
  });

  it('carries the description over by key', () => {
    const previous = [{ key: 'X-A', value: 'old', enabled: true, description: 'why' }];
    const out = entriesToHeaders([{ id: '0', key: 'X-A', value: 'new', enabled: true }], previous);
    expect(out).toEqual([{ key: 'X-A', value: 'new', enabled: true, description: 'why' }]);
  });

  it('adds no description key when the header had none', () => {
    const out = entriesToHeaders(
      [{ id: '0', key: 'X-A', value: '1', enabled: true }],
      [{ key: 'X-A', value: '1', enabled: true, description: null }],
    );
    expect('description' in out[0]).toBe(false);
  });
});

describe('folderChainPath', () => {
  it('is empty for the root folder', () => {
    expect(folderChainPath('')).toBe('');
  });

  it('is a synthetic file path inside the folder', () => {
    expect(folderChainPath('api/users')).toBe('api/users/folder.yml');
  });
});
