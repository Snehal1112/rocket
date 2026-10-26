import { describe, expect, it } from 'vitest';
import type { OpenApiSyncPlan } from '@/lib/tauri-api';
import { buildSelection, type SelectionState } from '../buildSelection';

const plan: OpenApiSyncPlan = {
  pending: true,
  items: [
    { kind: 'new', key: 'GET /a', name: 'A', digest: 'dA', fingerprint: 'fA', folder: [] },
    {
      kind: 'changed',
      key: 'GET /b',
      name: 'B',
      digest: 'dB',
      fingerprint: 'fB',
      requestPath: 'b.yml',
      cautious: false,
      changes: [
        { field: 'url', outcome: 'conflict' },
        { field: 'body', outcome: 'takeSpec' },
      ],
    },
    {
      kind: 'removed',
      key: 'GET /c',
      name: 'C',
      digest: 'dC',
      fingerprint: 'fC',
      requestPath: 'c.yml',
      edited: false,
    },
  ],
  variableChanges: [{ name: 'baseUrl', kind: 'baseUrl', digest: 'dV' }],
  unchanged: [],
  skippedHidden: 0,
  deletedByUser: [],
  cautious: false,
  warnings: [],
};

const empty = (): SelectionState => ({
  approved: new Set(),
  choices: new Map(),
  skips: new Set(),
  variables: new Set(),
  links: [],
});

describe('buildSelection', () => {
  it('is empty for an empty state', () => {
    expect(buildSelection(plan, empty())).toEqual({
      items: [],
      skips: [],
      variables: [],
      renameLinks: [],
    });
  });

  it('passes digests and fingerprints through untouched and omits unapproved items', () => {
    const state = empty();
    state.approved = new Set(['GET /a', 'GET /c']);
    state.skips = new Set(['GET /b']);
    state.variables = new Set(['baseUrl']);
    const out = buildSelection(plan, state);
    expect(out.items).toEqual([
      { key: 'GET /a', digest: 'dA', choices: {} },
      { key: 'GET /c', digest: 'dC', choices: {} },
    ]);
    expect(out.skips).toEqual([{ key: 'GET /b', fingerprint: 'fB' }]);
    expect(out.variables).toEqual([{ name: 'baseUrl', digest: 'dV' }]);
  });

  it('includes only the chosen field choices of an approved item', () => {
    const state = empty();
    state.approved = new Set(['GET /b']);
    state.choices = new Map([
      ['GET /b', { url: 'spec' }],
      ['GET /a', { url: 'yours' }],
    ]);
    const out = buildSelection(plan, state);
    expect(out.items).toEqual([{ key: 'GET /b', digest: 'dB', choices: { url: 'spec' } }]);
  });

  it('keeps the link order', () => {
    const state = empty();
    state.links = [
      ['z', 'y'],
      ['a', 'b'],
    ];
    expect(buildSelection(plan, state).renameLinks).toEqual([
      ['z', 'y'],
      ['a', 'b'],
    ]);
  });

  it('never invents a key that is not in the plan', () => {
    const state = empty();
    state.approved = new Set(['GET /nope']);
    state.skips = new Set(['GET /nope']);
    state.variables = new Set(['nope']);
    const out = buildSelection(plan, state);
    expect(out.items).toEqual([]);
    expect(out.skips).toEqual([]);
    expect(out.variables).toEqual([]);
  });

  it('follows the plan order, not the set order', () => {
    const state = empty();
    state.approved = new Set(['GET /c', 'GET /a']);
    expect(buildSelection(plan, state).items.map((i) => i.key)).toEqual(['GET /a', 'GET /c']);
  });

  it('drops choices for fields that are not in the current changes', () => {
    const state = empty();
    state.approved = new Set(['GET /b', 'GET /a']);
    state.choices = new Map<string, Record<string, 'spec' | 'yours'>>([
      ['GET /b', { url: 'spec', gone: 'yours' }],
      ['GET /a', { url: 'spec' }],
    ]);
    const out = buildSelection(plan, state);
    expect(out.items).toEqual([
      { key: 'GET /a', digest: 'dA', choices: {} },
      { key: 'GET /b', digest: 'dB', choices: { url: 'spec' } },
    ]);
  });

  it('never sends an item as both approved and skipped, the skip wins', () => {
    const state = empty();
    state.approved = new Set(['GET /a']);
    state.skips = new Set(['GET /a']);
    const out = buildSelection(plan, state);
    expect(out.items).toEqual([]);
    expect(out.skips).toEqual([{ key: 'GET /a', fingerprint: 'fA' }]);
  });
});
