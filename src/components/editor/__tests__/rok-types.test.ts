import { describe, expect, it } from 'vitest';
import type { ScriptSnippetGroup } from '../rok-types';
import {
  POST_RESPONSE_SNIPPETS,
  PRE_REQUEST_SNIPPETS,
  ROK_SNIPPETS,
  ROK_TYPE_DEFS_FOR_PHASE,
} from '../rok-types';

function rokItemLabels(groups: ScriptSnippetGroup[]): string[] {
  const group = groups.find((g) => g.id === 'api-reference');
  const sub = group?.subGroups?.find((s) => s.id === 'rok');
  return (sub?.items ?? []).map((i) => i.label);
}

describe('ROK_SNIPPETS', () => {
  it('has a common-tests group with at least 7 items', () => {
    const group = ROK_SNIPPETS.find((g) => g.id === 'common-tests');
    expect(group).toBeDefined();
    expect(group?.items?.length ?? 0).toBeGreaterThanOrEqual(7);
  });

  it('has an api-reference group with res, rok, expect sub-groups', () => {
    const group = ROK_SNIPPETS.find((g) => g.id === 'api-reference');
    expect(group).toBeDefined();
    const ids = (group?.subGroups ?? []).map((s) => s.id);
    expect(ids).toContain('res');
    expect(ids).toContain('rok');
    expect(ids).toContain('expect');
  });

  it('every snippet item has label, code, and kind', () => {
    for (const group of ROK_SNIPPETS) {
      for (const item of group.items ?? []) {
        expect(item.label).toBeTruthy();
        expect(item.code).toBeTruthy();
        expect(['template', 'expression']).toContain(item.kind);
      }
      for (const sub of group.subGroups ?? []) {
        for (const item of sub.items) {
          expect(item.label).toBeTruthy();
          expect(item.code).toBeTruthy();
          expect(['template', 'expression']).toContain(item.kind);
        }
      }
    }
  });
});

describe('rok.runner.* snippet coverage', () => {
  // setNextRequest is meaningful from any phase (spec §4: checked after every
  // phase that ran), so it belongs in all three snippet lists.
  it('setNextRequest appears in the tests-phase (default), pre-request, and post-response lists', () => {
    expect(rokItemLabels(ROK_SNIPPETS)).toContain('rok.runner.setNextRequest("name")');
    expect(rokItemLabels(PRE_REQUEST_SNIPPETS)).toContain('rok.runner.setNextRequest("name")');
    expect(rokItemLabels(POST_RESPONSE_SNIPPETS)).toContain('rok.runner.setNextRequest("name")');
  });

  // skipRequest only has an effect when read from the before-request phase
  // (spec §4: no HTTP call is made, so nothing later ever runs) — it must
  // not appear in the post-response or tests snippet lists.
  it('skipRequest appears only in the pre-request list', () => {
    expect(rokItemLabels(PRE_REQUEST_SNIPPETS)).toContain('rok.runner.skipRequest()');
    expect(rokItemLabels(ROK_SNIPPETS)).not.toContain('rok.runner.skipRequest()');
    expect(rokItemLabels(POST_RESPONSE_SNIPPETS)).not.toContain('rok.runner.skipRequest()');
  });
});

describe('ROK_TYPE_DEFS_FOR_PHASE', () => {
  it('returns a non-empty string for each phase', () => {
    expect(ROK_TYPE_DEFS_FOR_PHASE('pre-request').length).toBeGreaterThan(0);
    expect(ROK_TYPE_DEFS_FOR_PHASE('post-response').length).toBeGreaterThan(0);
    expect(ROK_TYPE_DEFS_FOR_PHASE('tests').length).toBeGreaterThan(0);
  });

  it('tests phase includes test() and expect declarations', () => {
    const defs = ROK_TYPE_DEFS_FOR_PHASE('tests');
    expect(defs).toContain('declare function test(');
    expect(defs).toContain('declare const expect');
  });

  it('pre-request phase includes req but not res or test', () => {
    const defs = ROK_TYPE_DEFS_FOR_PHASE('pre-request');
    expect(defs).toContain('declare const req');
    expect(defs).not.toContain('declare const res');
    expect(defs).not.toContain('declare function test(');
  });

  it('post-response phase includes res but not req or test', () => {
    const defs = ROK_TYPE_DEFS_FOR_PHASE('post-response');
    expect(defs).toContain('declare const res');
    expect(defs).not.toContain('declare const req');
    expect(defs).not.toContain('declare function test(');
  });
});
