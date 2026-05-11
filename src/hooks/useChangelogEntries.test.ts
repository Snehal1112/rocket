import { renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { useDrawerStore } from '@/stores/contracts/drawerSlice';
import type { ChangelogEntry, Contract } from '@/types/contracts';
import { DEFAULT_FILTERS } from '@/types/contracts';
import { useChangelogEntries } from './useChangelogEntries';

function makeEntry(overrides: Partial<ChangelogEntry> & { id: string }): ChangelogEntry {
  return {
    contractId: 'c1',
    at: '2025-01-15T10:00:00Z',
    kind: 'add',
    summary: 'param added',
    isBreaking: false,
    ...overrides,
  };
}

const baseContract: Contract = {
  id: 'c1',
  collectionId: 'col1',
  name: 'Test Contract',
  version: '1.0.0',
  status: 'active',
  provider: { id: 'p1', name: 'Provider', kind: 'service' },
  consumers: [],
  scope: { type: 'collection' },
  policy: { breakingChangePolicy: 'strict', noticeDays: 7, uptimeSla: 99 },
  effectiveAt: '2025-01-10',
  expiresAt: null,
  signedSnapshot: null,
  driftCount: 0,
  breachCount: 0,
  endpointCount: 0,
  changelog: [],
  createdBy: 'user1',
  createdAt: '2025-01-01T00:00:00Z',
  updatedAt: '2025-01-01T00:00:00Z',
};

describe('useChangelogEntries', () => {
  beforeEach(() => {
    useDrawerStore.setState({
      isOpen: true,
      contractId: 'c1',
      filters: { ...DEFAULT_FILTERS },
    });
  });

  it('returns all entries when no filters are active', () => {
    const contract = {
      ...baseContract,
      changelog: [
        makeEntry({ id: 'e1', kind: 'add' }),
        makeEntry({ id: 'e2', kind: 'remove' }),
        makeEntry({ id: 'e3', kind: 'modify' }),
      ],
    };
    const { result } = renderHook(() => useChangelogEntries(contract));
    const allItems = result.current.flatMap((g) => g.items);
    expect(allItems).toHaveLength(3);
  });

  it('kind filter returns only matching entries', () => {
    const contract = {
      ...baseContract,
      changelog: [
        makeEntry({ id: 'e1', kind: 'add' }),
        makeEntry({ id: 'e2', kind: 'remove' }),
        makeEntry({ id: 'e3', kind: 'add' }),
      ],
    };
    useDrawerStore.setState({ filters: { ...DEFAULT_FILTERS, kinds: ['add'] } });
    const { result } = renderHook(() => useChangelogEntries(contract));
    const allItems = result.current.flatMap((g) => g.items);
    expect(allItems).toHaveLength(2);
    expect(allItems.every((e) => e.kind === 'add')).toBe(true);
  });

  it('breakingOnly filter returns only breaking entries', () => {
    const contract = {
      ...baseContract,
      changelog: [
        makeEntry({ id: 'e1', isBreaking: true }),
        makeEntry({ id: 'e2', isBreaking: false }),
        makeEntry({ id: 'e3', isBreaking: true }),
      ],
    };
    useDrawerStore.setState({ filters: { ...DEFAULT_FILTERS, breakingOnly: true } });
    const { result } = renderHook(() => useChangelogEntries(contract));
    const allItems = result.current.flatMap((g) => g.items);
    expect(allItems).toHaveLength(2);
    expect(allItems.every((e) => e.isBreaking)).toBe(true);
  });

  it('search filters on summary', () => {
    const contract = {
      ...baseContract,
      changelog: [
        makeEntry({ id: 'e1', summary: 'query.limit removed' }),
        makeEntry({ id: 'e2', summary: 'headers updated' }),
      ],
    };
    useDrawerStore.setState({ filters: { ...DEFAULT_FILTERS, search: 'limit' } });
    const { result } = renderHook(() => useChangelogEntries(contract));
    const allItems = result.current.flatMap((g) => g.items);
    expect(allItems).toHaveLength(1);
    expect(allItems[0].id).toBe('e1');
  });

  it('search filters on requestPath', () => {
    const contract = {
      ...baseContract,
      changelog: [
        makeEntry({ id: 'e1', requestPath: '/api/users' }),
        makeEntry({ id: 'e2', requestPath: '/api/orders' }),
      ],
    };
    useDrawerStore.setState({ filters: { ...DEFAULT_FILTERS, search: 'users' } });
    const { result } = renderHook(() => useChangelogEntries(contract));
    const allItems = result.current.flatMap((g) => g.items);
    expect(allItems).toHaveLength(1);
    expect(allItems[0].id).toBe('e1');
  });

  it('search filters on authorName', () => {
    const contract = {
      ...baseContract,
      changelog: [
        makeEntry({ id: 'e1', authorName: 'Alice' }),
        makeEntry({ id: 'e2', authorName: 'Bob' }),
      ],
    };
    useDrawerStore.setState({ filters: { ...DEFAULT_FILTERS, search: 'alice' } });
    const { result } = renderHook(() => useChangelogEntries(contract));
    const allItems = result.current.flatMap((g) => g.items);
    expect(allItems).toHaveLength(1);
    expect(allItems[0].id).toBe('e1');
  });

  it('combined filter: kinds + breakingOnly', () => {
    const contract = {
      ...baseContract,
      changelog: [
        makeEntry({ id: 'e1', kind: 'add', isBreaking: true }),
        makeEntry({ id: 'e2', kind: 'add', isBreaking: false }),
        makeEntry({ id: 'e3', kind: 'remove', isBreaking: true }),
      ],
    };
    useDrawerStore.setState({
      filters: { ...DEFAULT_FILTERS, kinds: ['add'], breakingOnly: true },
    });
    const { result } = renderHook(() => useChangelogEntries(contract));
    const allItems = result.current.flatMap((g) => g.items);
    expect(allItems).toHaveLength(1);
    expect(allItems[0].id).toBe('e1');
  });

  it('combined filter: kinds + search', () => {
    const contract = {
      ...baseContract,
      changelog: [
        makeEntry({ id: 'e1', kind: 'add', summary: 'new endpoint added' }),
        makeEntry({ id: 'e2', kind: 'add', summary: 'param added' }),
        makeEntry({ id: 'e3', kind: 'remove', summary: 'new field removed' }),
      ],
    };
    useDrawerStore.setState({ filters: { ...DEFAULT_FILTERS, kinds: ['add'], search: 'new' } });
    const { result } = renderHook(() => useChangelogEntries(contract));
    const allItems = result.current.flatMap((g) => g.items);
    expect(allItems).toHaveLength(1);
    expect(allItems[0].id).toBe('e1');
  });

  it('since-signed filter excludes entries before effectiveAt', () => {
    const contract = {
      ...baseContract,
      effectiveAt: '2025-01-10',
      changelog: [
        makeEntry({ id: 'e1', at: '2025-01-15T10:00:00Z' }),
        makeEntry({ id: 'e2', at: '2025-01-09T10:00:00Z' }),
        makeEntry({ id: 'e3', at: '2025-01-10T00:00:00Z' }),
      ],
    };
    useDrawerStore.setState({ filters: { ...DEFAULT_FILTERS, sinceSigned: true } });
    const { result } = renderHook(() => useChangelogEntries(contract));
    const allItems = result.current.flatMap((g) => g.items);
    expect(allItems).toHaveLength(2);
    expect(allItems.map((e) => e.id)).not.toContain('e2');
  });

  it('sorts entries most recent first', () => {
    const contract = {
      ...baseContract,
      changelog: [
        makeEntry({ id: 'e1', at: '2025-01-10T10:00:00Z' }),
        makeEntry({ id: 'e3', at: '2025-01-15T10:00:00Z' }),
        makeEntry({ id: 'e2', at: '2025-01-12T10:00:00Z' }),
      ],
    };
    const { result } = renderHook(() => useChangelogEntries(contract));
    const allItems = result.current.flatMap((g) => g.items);
    expect(allItems[0].id).toBe('e3');
    expect(allItems[1].id).toBe('e2');
    expect(allItems[2].id).toBe('e1');
  });

  it('groupByDay produces correct labels for today and yesterday', () => {
    const today = new Date().toLocaleDateString('en-CA');
    const yesterdayDate = new Date();
    yesterdayDate.setDate(yesterdayDate.getDate() - 1);
    const yesterday = yesterdayDate.toLocaleDateString('en-CA');

    const contract = {
      ...baseContract,
      changelog: [
        makeEntry({ id: 'e1', at: `${today}T10:00:00Z` }),
        makeEntry({ id: 'e2', at: `${yesterday}T10:00:00Z` }),
        makeEntry({ id: 'e3', at: '2024-06-01T10:00:00Z' }),
      ],
    };
    const { result } = renderHook(() => useChangelogEntries(contract));
    expect(result.current[0].label).toMatch(/^Today ·/);
    expect(result.current[1].label).toMatch(/^Yesterday ·/);
    expect(result.current[2].label).not.toMatch(/^Today|Yesterday/);
  });
});
