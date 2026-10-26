import type { OpenApiSyncItem, OpenApiSyncPlan, OpenApiSyncStatus } from '@/lib/tauri-api';

export function makeStatus(overrides: Partial<OpenApiSyncStatus> = {}): OpenApiSyncStatus {
  return {
    location: 'https://example.com/openapi.json',
    interval: 'daily',
    lastCheckedAt: new Date(Date.now() - 2 * 3_600_000).toISOString(),
    lastStatus: { state: 'changed' },
    hasPending: true,
    hasToken: false,
    consent: 'granted',
    ...overrides,
  };
}

export const newItem: OpenApiSyncItem = {
  kind: 'new',
  key: 'GET /pets',
  name: 'List pets',
  digest: 'd1',
  fingerprint: 'f1',
  folder: ['pets', 'read'],
  possiblyRenamedFrom: 'GET /animals',
};

export const changedItem: OpenApiSyncItem = {
  kind: 'changed',
  key: 'POST /pets',
  name: 'Add pet',
  digest: 'd2',
  fingerprint: 'f2',
  requestPath: 'pets/add-pet.yml',
  cautious: false,
  changes: [
    { field: 'header:Accept', outcome: 'takeSpec', baseline: 'a/b', yours: 'a/b', spec: 'a/c' },
    { field: 'body:$.a.b', outcome: 'keepYours', baseline: '1', yours: '2', spec: '3' },
    { field: 'url', outcome: 'conflict', baseline: 'u1', yours: 'u2', spec: 'u3' },
  ],
};

export const removedItem: OpenApiSyncItem = {
  kind: 'removed',
  key: 'DELETE /pets',
  name: 'Delete pet',
  digest: 'd3',
  fingerprint: 'f3',
  requestPath: 'pets/delete-pet.yml',
  edited: null,
  possiblyRenamedTo: 'DELETE /animals',
};

export function makePlan(overrides: Partial<OpenApiSyncPlan> = {}): OpenApiSyncPlan {
  return {
    pending: true,
    items: [removedItem, changedItem, newItem],
    variableChanges: [],
    unchanged: [],
    skippedHidden: 0,
    deletedByUser: [],
    cautious: false,
    warnings: [],
    ...overrides,
  };
}
