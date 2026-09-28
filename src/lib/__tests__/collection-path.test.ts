import { describe, expect, it } from 'vitest';
import type { CollectionReference } from '@/lib/tauri-api';
import { resolveCollectionPath } from '../collection-path';

describe('resolveCollectionPath', () => {
  it('concatenates the workspace path for an embedded collection', () => {
    const collections: CollectionReference[] = [{ name: 'my-collection', type: 'embedded' }];
    const result = resolveCollectionPath('my-collection', '/ws/root', collections);
    expect(result).toBe('/ws/root/collections/my-collection');
  });

  it('uses the CollectionReference path directly for an external collection', () => {
    const collections: CollectionReference[] = [
      { name: 'ext-collection', type: 'external', path: '/somewhere/else' },
    ];
    const result = resolveCollectionPath('ext-collection', '/ws/root', collections);
    expect(result).toBe('/somewhere/else');
  });

  it('falls back to the workspace concatenation when no matching reference is found', () => {
    const result = resolveCollectionPath('unknown-collection', '/ws/root', []);
    expect(result).toBe('/ws/root/collections/unknown-collection');
  });

  it('falls back to the workspace concatenation when an external reference has no path', () => {
    const collections: CollectionReference[] = [{ name: 'ext-collection', type: 'external' }];
    const result = resolveCollectionPath('ext-collection', '/ws/root', collections);
    expect(result).toBe('/ws/root/collections/ext-collection');
  });
});
