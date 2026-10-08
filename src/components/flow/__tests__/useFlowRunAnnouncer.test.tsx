import { renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { FlowNode, FlowNodeStatus } from '@/lib/tauri-api';
import type { FlowNodeDetail } from '@/types/pane-types';
import { useFlowRunAnnouncer } from '../useFlowRunAnnouncer';

const nodes: FlowNode[] = [
  { id: 'a', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Alpha' } },
];

interface Props {
  status: Record<string, FlowNodeStatus>;
  detail?: Record<string, FlowNodeDetail>;
  runState: 'idle' | 'running' | 'done';
}

function setup(initial: Props) {
  return renderHook((p: Props) => useFlowRunAnnouncer(nodes, p.status, p.detail, p.runState), {
    initialProps: initial,
  });
}

describe('useFlowRunAnnouncer', () => {
  it('is silent on mount, even when the tab already holds a finished run', () => {
    const { result } = setup({ status: { a: 'success' }, runState: 'done' });
    expect(result.current).toEqual({ polite: '', alert: '' });
  });

  it('is silent when mounted in the middle of a run', () => {
    const { result } = setup({ status: {}, runState: 'running' });
    expect(result.current).toEqual({ polite: '', alert: '' });
  });

  it('announces a run from start to finish', () => {
    const { result, rerender } = setup({ status: {}, runState: 'idle' });
    rerender({ status: {}, runState: 'running' });
    expect(result.current.polite).toBe('Run started.');
    rerender({ status: { a: 'success' }, runState: 'running' });
    expect(result.current.polite).toBe('Alpha succeeded.');
    rerender({ status: { a: 'success' }, runState: 'done' });
    expect(result.current.polite).toBe('Run finished: 1 succeeded, 0 failed, 0 skipped.');
  });

  it('says "Run started." again for a second run, with other text in between', () => {
    const { result, rerender } = setup({ status: {}, runState: 'idle' });
    rerender({ status: {}, runState: 'running' });
    const first = result.current.polite;
    rerender({ status: { a: 'success' }, runState: 'done' });
    expect(result.current.polite).not.toBe(first);
    rerender({ status: {}, runState: 'running' });
    expect(result.current.polite).toBe(first);
  });

  it('puts a failure in the alert text and clears it when the next run starts', () => {
    const { result, rerender } = setup({ status: {}, runState: 'running' });
    rerender({ status: { a: 'failed' }, detail: { a: { error: 'boom' } }, runState: 'running' });
    expect(result.current.alert).toBe('Alpha failed: boom.');
    rerender({ status: { a: 'failed' }, detail: { a: { error: 'boom' } }, runState: 'done' });
    rerender({ status: {}, runState: 'running' });
    expect(result.current.alert).toBe('');
  });

  it('keeps the last text when only progress details change', () => {
    const { result, rerender } = setup({ status: {}, runState: 'idle' });
    rerender({ status: {}, runState: 'running' });
    rerender({ status: {}, detail: { a: { progress: 'attempt 2/5' } }, runState: 'running' });
    expect(result.current.polite).toBe('Run started.');
  });
});
