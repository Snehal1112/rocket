import { describe, expect, it } from 'vitest';
import type { Contract } from '@/types/contracts';
import { computeDrift } from './drift';

function makeContract(overrides: Partial<Contract> = {}): Contract {
  return {
    id: 'c1',
    collectionId: 'col1',
    name: 'Payments API',
    version: '1.0.0',
    status: 'active',
    provider: { id: 'p1', name: 'Billing', kind: 'team' },
    consumers: [{ id: 'c1', name: 'Platform', kind: 'team' }],
    scope: { type: 'collection' },
    policy: { breakingChangePolicy: 'lenient', noticeDays: 30, uptimeSla: null },
    effectiveAt: '2026-01-01',
    expiresAt: null,
    signedSnapshot: {
      r1: {
        method: 'GET',
        path: '/payments',
        params: [
          { key: 'currency', required: true },
          { key: 'page', required: false },
        ],
        headers: [{ key: 'Authorization', required: true }],
      },
    },
    driftCount: 0,
    breachCount: 0,
    endpointCount: 1,
    changelog: [],
    createdBy: 'user1',
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    ...overrides,
  };
}

const currentRequests = [
  {
    id: 'r1',
    method: 'GET',
    path: '/payments',
    params: [
      { key: 'currency', required: true },
      { key: 'page', required: false },
    ],
    headers: [{ key: 'Authorization', required: true }],
    body: null,
    folderId: 'root',
  },
];

describe('computeDrift — no changes', () => {
  it('returns zero counts when snapshot matches current', () => {
    const report = computeDrift(makeContract(), currentRequests);
    expect(report.driftCount).toBe(0);
    expect(report.breachCount).toBe(0);
    expect(report.diffs).toHaveLength(0);
  });
});

describe('computeDrift — method change (always breaking)', () => {
  it('detects method change as breaking for all policies', () => {
    const requests = [{ ...currentRequests[0], method: 'POST' }];
    for (const policy of ['strict', 'lenient', 'additive_ok'] as const) {
      const contract = makeContract({
        policy: { breakingChangePolicy: policy, noticeDays: 30, uptimeSla: null },
      });
      const report = computeDrift(contract, requests);
      expect(report.breachCount).toBeGreaterThan(0);
      const change = report.diffs[0]?.changes.find((c) => c.field === 'method');
      expect(change?.isBreaking).toBe(true);
    }
  });
});

describe('computeDrift — path change (always breaking)', () => {
  it('detects path change as breaking', () => {
    const requests = [{ ...currentRequests[0], path: '/v2/payments' }];
    const report = computeDrift(makeContract(), requests);
    const change = report.diffs[0]?.changes.find((c) => c.field === 'path');
    expect(change?.isBreaking).toBe(true);
  });
});

describe('computeDrift — required param removed', () => {
  it('is always breaking regardless of policy', () => {
    const requests = [{ ...currentRequests[0], params: [{ key: 'page', required: false }] }];
    for (const policy of ['strict', 'lenient', 'additive_ok'] as const) {
      const contract = makeContract({
        policy: { breakingChangePolicy: policy, noticeDays: 30, uptimeSla: null },
      });
      const report = computeDrift(contract, requests);
      const change = report.diffs[0]?.changes.find((c) => c.field === 'params.currency');
      expect(change?.isBreaking).toBe(true);
    }
  });
});

describe('computeDrift — optional param removed', () => {
  it('is breaking for strict and lenient, not for additive_ok', () => {
    const requests = [{ ...currentRequests[0], params: [{ key: 'currency', required: true }] }];

    const strict = computeDrift(
      makeContract({ policy: { breakingChangePolicy: 'strict', noticeDays: 30, uptimeSla: null } }),
      requests,
    );
    expect(strict.diffs[0]?.changes.find((c) => c.field === 'params.page')?.isBreaking).toBe(true);

    const lenient = computeDrift(
      makeContract({
        policy: { breakingChangePolicy: 'lenient', noticeDays: 30, uptimeSla: null },
      }),
      requests,
    );
    expect(lenient.diffs[0]?.changes.find((c) => c.field === 'params.page')?.isBreaking).toBe(true);

    const additive = computeDrift(
      makeContract({
        policy: { breakingChangePolicy: 'additive_ok', noticeDays: 30, uptimeSla: null },
      }),
      requests,
    );
    expect(additive.diffs[0]?.changes.find((c) => c.field === 'params.page')?.isBreaking).toBe(
      false,
    );
  });
});

describe('computeDrift — new param added', () => {
  it('is breaking only for strict policy', () => {
    const requests = [
      {
        ...currentRequests[0],
        params: [...currentRequests[0].params, { key: 'format', required: false }],
      },
    ];

    const strict = computeDrift(
      makeContract({ policy: { breakingChangePolicy: 'strict', noticeDays: 30, uptimeSla: null } }),
      requests,
    );
    expect(strict.diffs[0]?.changes.find((c) => c.field === 'params.format')?.isBreaking).toBe(
      true,
    );

    const lenient = computeDrift(
      makeContract({
        policy: { breakingChangePolicy: 'lenient', noticeDays: 30, uptimeSla: null },
      }),
      requests,
    );
    expect(lenient.diffs[0]?.changes.find((c) => c.field === 'params.format')?.isBreaking).toBe(
      false,
    );

    const additive = computeDrift(
      makeContract({
        policy: { breakingChangePolicy: 'additive_ok', noticeDays: 30, uptimeSla: null },
      }),
      requests,
    );
    expect(additive.diffs[0]?.changes.find((c) => c.field === 'params.format')?.isBreaking).toBe(
      false,
    );
  });
});

describe('computeDrift — entire request removed', () => {
  it('is always breaking', () => {
    const report = computeDrift(makeContract(), []);
    expect(report.breachCount).toBeGreaterThan(0);
    const change = report.diffs[0]?.changes.find((c) => c.field === 'request');
    expect(change?.kind).toBe('remove');
    expect(change?.isBreaking).toBe(true);
  });
});

describe('computeDrift — new request added', () => {
  it('is breaking only for strict policy', () => {
    const requests = [
      ...currentRequests,
      {
        id: 'r2',
        method: 'DELETE',
        path: '/payments/:id',
        params: [],
        headers: [],
        body: null,
        folderId: 'root',
      },
    ];

    const strict = computeDrift(
      makeContract({ policy: { breakingChangePolicy: 'strict', noticeDays: 30, uptimeSla: null } }),
      requests,
    );
    expect(strict.diffs.find((d) => d.requestId === 'r2')?.changes[0].isBreaking).toBe(true);

    const lenient = computeDrift(
      makeContract({
        policy: { breakingChangePolicy: 'lenient', noticeDays: 30, uptimeSla: null },
      }),
      requests,
    );
    expect(lenient.diffs.find((d) => d.requestId === 'r2')?.changes[0].isBreaking).toBe(false);
  });
});

describe('computeDrift — draft contract', () => {
  it('returns empty report when signedSnapshot is null (draft)', () => {
    const draft = makeContract({ status: 'draft', signedSnapshot: null });
    const report = computeDrift(draft, currentRequests);
    expect(report.driftCount).toBe(0);
    expect(report.diffs).toHaveLength(0);
  });
});

describe('computeDrift — paused contract', () => {
  it('returns empty report (paused contracts are skipped)', () => {
    const paused = makeContract({ status: 'paused' });
    const report = computeDrift(paused, currentRequests);
    expect(report.driftCount).toBe(0);
  });
});
