import { describe, expect, it } from 'vitest';
import { sortItemsFoldersFirst } from '../collection-utils';
import type { CollectionItem } from '../tauri-api';

// Minimal helpers to build test fixtures without filling every field.
const folder = (name: string): CollectionItem =>
  ({ type: 'folder', uid: name, name, items: [] }) as CollectionItem;

const request = (name: string): CollectionItem =>
  ({
    type: 'request',
    uid: name,
    name,
    method: 'GET',
    url: '',
    headers: [],
    body: { mode: 'none' },
    auth: { type: 'none' },
  }) as CollectionItem;

describe('sortItemsFoldersFirst', () => {
  it('returns empty array unchanged', () => {
    expect(sortItemsFoldersFirst([])).toEqual([]);
  });

  it('sorts all folders alphabetically', () => {
    const result = sortItemsFoldersFirst([folder('Zebra'), folder('alpha'), folder('Mango')]);
    expect(result.map((i) => i.name)).toEqual(['alpha', 'Mango', 'Zebra']);
  });

  it('sorts all requests alphabetically', () => {
    const result = sortItemsFoldersFirst([request('Zebra'), request('alpha'), request('Mango')]);
    expect(result.map((i) => i.name)).toEqual(['alpha', 'Mango', 'Zebra']);
  });

  it('places all folders before all requests', () => {
    const result = sortItemsFoldersFirst([
      request('A Request'),
      folder('Z Folder'),
      request('B Request'),
      folder('A Folder'),
    ]);
    expect(result.map((i) => i.name)).toEqual(['A Folder', 'Z Folder', 'A Request', 'B Request']);
  });

  it('sorts case-insensitively within each group', () => {
    const result = sortItemsFoldersFirst([folder('zebra'), folder('Alpha'), folder('MANGO')]);
    expect(result.map((i) => i.name)).toEqual(['Alpha', 'MANGO', 'zebra']);
  });

  it('does not mutate the input array', () => {
    const input = [request('B'), folder('A')];
    const copy = [...input];
    sortItemsFoldersFirst(input);
    expect(input).toEqual(copy);
  });
});
