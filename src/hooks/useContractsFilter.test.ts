import { describe, expect, it } from 'vitest';
import type { Contract, ContractsFilterState } from '@/types/contracts';
import { applyFilter } from './useContractsFilter';

function c(id: string, status: Contract['status'], name = `Contract ${id}`): Contract {
  return {
    id,
    collectionId: 'col1',
    name,
    version: '1.0.0',
    status,
    provider: { id: 'p', name: 'Billing Team', kind: 'team' },
    consumers: [{ id: 'c', name: 'Platform', kind: 'team' }],
    scope: { type: 'collection' },
    policy: { breakingChangePolicy: 'lenient', noticeDays: 30, uptimeSla: null },
    effectiveAt: '2026-01-01',
    expiresAt: null,
    signedSnapshot: null,
    driftCount: 0,
    breachCount: 0,
    endpointCount: 1,
    changelog: [],
    createdBy: 'u1',
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
  };
}

const contracts = [
  c('r1', 'active', 'Payments API'),
  c('r2', 'drift'),
  c('r3', 'breach'),
  c('r4', 'draft', 'Orders API'),
];

const base: ContractsFilterState = {
  search: '',
  statuses: ['all'],
  sort: 'updated',
  sortDir: 'desc',
  view: 'cards',
};

describe('applyFilter', () => {
  it('all filter returns all contracts', () => {
    expect(applyFilter(contracts, base)).toHaveLength(4);
  });
  it('status filter returns only matching status', () => {
    const result = applyFilter(contracts, { ...base, statuses: ['drift'] });
    expect(result).toHaveLength(1);
    expect(result[0].id).toBe('r2');
  });
  it('multiple status chips are OR combined', () => {
    const result = applyFilter(contracts, { ...base, statuses: ['drift', 'breach'] });
    expect(result).toHaveLength(2);
  });
  it('search filters by name', () => {
    const result = applyFilter(contracts, { ...base, search: 'Payments' });
    expect(result).toHaveLength(1);
    expect(result[0].id).toBe('r1');
  });
  it('search filters by provider name', () => {
    const result = applyFilter(contracts, { ...base, search: 'Billing' });
    expect(result).toHaveLength(4); // all share same provider
  });
  it('empty search returns all', () => {
    expect(applyFilter(contracts, { ...base, search: '' })).toHaveLength(4);
  });
});
