import { describe, expect, it } from 'vitest';
import type { FlowNode, FlowNodeStatus } from '@/lib/tauri-api';
import { type AnnounceSnapshot, diffAnnouncements, MAX_NODE_MESSAGES } from '../flowAnnounce';

const nodes: FlowNode[] = [
  { id: 'a', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Alpha' } },
  { id: 'b', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Beta' } },
  { id: 'c', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Gamma' } },
];

const snap = (
  runState: AnnounceSnapshot['runState'],
  status: Record<string, FlowNodeStatus> = {},
): AnnounceSnapshot => ({ runState, status });

describe('diffAnnouncements', () => {
  it('says nothing when nothing changed', () => {
    const s = snap('done', { a: 'success' });
    expect(diffAnnouncements(s, s, nodes)).toEqual({ polite: [], alerts: [], runStarted: false });
  });

  it('announces the start of a run once', () => {
    const out = diffAnnouncements(snap('idle'), snap('running'), nodes);
    expect(out.polite).toEqual(['Run started.']);
    expect(out.runStarted).toBe(true);
    expect(diffAnnouncements(snap('running'), snap('running'), nodes).polite).toEqual([]);
  });

  it('does not announce a node that is merely running', () => {
    const out = diffAnnouncements(snap('running'), snap('running', { a: 'running' }), nodes);
    expect(out.polite).toEqual([]);
    expect(out.alerts).toEqual([]);
  });

  it('announces a node that succeeded or was skipped, with the reason', () => {
    const out = diffAnnouncements(
      snap('running', { a: 'running' }),
      snap('running', { a: 'success', b: 'skipped' }),
      nodes,
      { b: { skipReason: 'branch_not_taken' } },
    );
    expect(out.polite).toEqual(['Alpha succeeded.', 'Beta skipped, branch not taken.']);
  });

  it('sends a failure to the alert list with the short error', () => {
    const out = diffAnnouncements(snap('running'), snap('running', { a: 'failed' }), nodes, {
      a: { error: 'boom\nstack trace' },
    });
    expect(out.alerts).toEqual(['Alpha failed: boom.']);
    expect(out.polite).toEqual([]);
  });

  it('announces each terminal change only once', () => {
    const before = snap('running', { a: 'success' });
    const after = snap('running', { a: 'success', b: 'success' });
    expect(diffAnnouncements(before, after, nodes).polite).toEqual(['Beta succeeded.']);
  });

  it('ignores nodes that are not on the canvas', () => {
    const out = diffAnnouncements(snap('running'), snap('running', { ghost: 'success' }), nodes);
    expect(out.polite).toEqual([]);
  });

  it('summarises a finished run', () => {
    const out = diffAnnouncements(
      snap('running', { a: 'success', b: 'failed', c: 'skipped' }),
      snap('done', { a: 'success', b: 'failed', c: 'skipped' }),
      nodes,
    );
    expect(out.polite).toEqual(['Run finished: 1 succeeded, 1 failed, 1 skipped.']);
  });

  it('collapses a burst of node results to the failures and the summary', () => {
    const many: FlowNode[] = Array.from({ length: MAX_NODE_MESSAGES + 3 }, (_, i) => ({
      id: `n${i}`,
      position: { x: 0, y: 0 },
      kind: { kind: 'Output' as const, label: `Node ${i}` },
    }));
    const finalStatus: Record<string, FlowNodeStatus> = Object.fromEntries(
      many.map((n, i) => [n.id, i === 0 ? 'failed' : 'success']),
    );
    const out = diffAnnouncements(snap('running'), snap('done', finalStatus), many, {
      n0: { error: 'boom' },
    });
    expect(out.polite).toEqual([
      `Run finished: ${many.length - 1} succeeded, 1 failed, 0 skipped.`,
    ]);
    expect(out.alerts).toEqual(['Node 0 failed: boom.']);
  });

  it('caps the failure list and counts the rest', () => {
    const many: FlowNode[] = Array.from({ length: MAX_NODE_MESSAGES + 2 }, (_, i) => ({
      id: `n${i}`,
      position: { x: 0, y: 0 },
      kind: { kind: 'Output' as const, label: `Node ${i}` },
    }));
    const failed: Record<string, FlowNodeStatus> = Object.fromEntries(
      many.map((n) => [n.id, 'failed']),
    );
    const out = diffAnnouncements(snap('running'), snap('running', failed), many);
    expect(out.alerts).toHaveLength(MAX_NODE_MESSAGES + 1);
    expect(out.alerts[MAX_NODE_MESSAGES]).toBe('and 2 more failed.');
  });
});
