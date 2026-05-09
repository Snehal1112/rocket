import { describe, expect, it } from 'vitest';
import type { Contract } from '@/types/contracts';
import {
  groupContracts,
  selectContractCounts,
  selectContractsForCollection,
  sortContractsAttentionFirst,
} from './contractsSelectors';

function c(id: string, status: Contract['status'], driftCount = 0, breachCount = 0): Contract {
  return {
    id,
    collectionId: 'col1',
    name: `Contract ${id}`,
    version: '1.0.0',
    status,
    provider: { id: 'p', name: 'Provider', kind: 'team' },
    consumers: [{ id: 'c', name: 'Consumer', kind: 'team' }],
    scope: { type: 'collection' },
    policy: { breakingChangePolicy: 'lenient', noticeDays: 30, uptimeSla: null },
    effectiveAt: '2026-01-01',
    expiresAt: null,
    signedSnapshot: null,
    driftCount,
    breachCount,
    endpointCount: 1,
    changelog: [],
    createdBy: 'u1',
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
  };
}

const byId = {
  r1: c('r1', 'active'),
  r2: c('r2', 'drift', 3, 0),
  r3: c('r3', 'breach', 2, 1),
  r4: c('r4', 'draft'),
  r5: c('r5', 'paused'),
};
const byCollection = { col1: ['r1', 'r2', 'r3', 'r4', 'r5'] };

describe('selectContractsForCollection', () => {
  it('returns contracts for the given collection', () => {
    const result = selectContractsForCollection(byId, byCollection, 'col1');
    expect(result).toHaveLength(5);
  });
  it('returns empty array for unknown collection', () => {
    const result = selectContractsForCollection(byId, byCollection, 'unknown');
    expect(result).toHaveLength(0);
  });
});

describe('selectContractCounts', () => {
  it('counts all statuses correctly', () => {
    const contracts = Object.values(byId);
    const counts = selectContractCounts(contracts);
    expect(counts.total).toBe(5);
    expect(counts.active).toBe(1);
    expect(counts.drift).toBe(1);
    expect(counts.breach).toBe(1);
    expect(counts.draft).toBe(1);
    expect(counts.paused).toBe(1);
  });
  it('sums drift counts for totalChanges', () => {
    const contracts = Object.values(byId);
    const counts = selectContractCounts(contracts);
    // r2 has 3 drift, r3 has 2 drift
    expect(counts.totalChanges).toBe(5);
  });
});

describe('groupContracts', () => {
  it('puts breach and drift in attention group', () => {
    const contracts = Object.values(byId);
    const { attention, active, inactive } = groupContracts(contracts);
    expect(attention.map((c) => c.id).sort()).toEqual(['r2', 'r3'].sort());
    expect(active.map((c) => c.id)).toEqual(['r1']);
    expect(inactive.map((c) => c.id).sort()).toEqual(['r4', 'r5'].sort());
  });
});

describe('sortContractsAttentionFirst', () => {
  it('breach appears before drift before active', () => {
    const contracts = Object.values(byId);
    const sorted = sortContractsAttentionFirst(contracts);
    const statuses = sorted.map((c) => c.status);
    const breachIdx = statuses.indexOf('breach');
    const driftIdx = statuses.indexOf('drift');
    const activeIdx = statuses.indexOf('active');
    expect(breachIdx).toBeLessThan(driftIdx);
    expect(driftIdx).toBeLessThan(activeIdx);
  });
});
