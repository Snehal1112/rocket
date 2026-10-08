import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { type Flow, type FlowLint, lintFlow } from '@/lib/tauri-api';
import { useBackendFlowLints } from '../useBackendFlowLints';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, lintFlow: vi.fn() };
});

const flowNamed = (name: string): Flow => ({ name, nodes: [], edges: [] });
const lint = (code: string): FlowLint => ({ code, severity: 'warning', message: code });

describe('useBackendFlowLints', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.mocked(lintFlow).mockReset();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  const flush = () =>
    act(async () => {
      await vi.runAllTimersAsync();
    });

  it('lints once when a flow opens', async () => {
    vi.mocked(lintFlow).mockResolvedValue([lint('exit_without_edge')]);
    const { result } = renderHook(() => useBackendFlowLints('demo', flowNamed('a')));
    expect(lintFlow).not.toHaveBeenCalled();
    await flush();
    expect(lintFlow).toHaveBeenCalledTimes(1);
    expect(result.current.map((i) => i.code)).toEqual(['exit_without_edge']);
  });

  it('lints once after a pause in editing', async () => {
    vi.mocked(lintFlow).mockResolvedValue([]);
    const { rerender } = renderHook(({ flow }) => useBackendFlowLints('demo', flow), {
      initialProps: { flow: flowNamed('a') },
    });
    rerender({ flow: flowNamed('ab') });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(200);
    });
    rerender({ flow: flowNamed('abc') });
    await flush();
    expect(lintFlow).toHaveBeenCalledTimes(1);
    expect(lintFlow).toHaveBeenCalledWith('demo', flowNamed('abc'));
  });

  it('does not lint again when a render brings an equal graph', async () => {
    vi.mocked(lintFlow).mockResolvedValue([]);
    const { rerender } = renderHook(({ flow }) => useBackendFlowLints('demo', flow), {
      initialProps: { flow: flowNamed('a') },
    });
    await flush();
    rerender({ flow: flowNamed('a') });
    await flush();
    expect(lintFlow).toHaveBeenCalledTimes(1);
  });

  it('ignores a response that is out of date', async () => {
    let answerOld: (lints: FlowLint[]) => void = () => undefined;
    vi.mocked(lintFlow)
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            answerOld = resolve;
          }),
      )
      .mockResolvedValueOnce([lint('new')]);
    const { result, rerender } = renderHook(({ flow }) => useBackendFlowLints('demo', flow), {
      initialProps: { flow: flowNamed('a') },
    });
    await flush();
    rerender({ flow: flowNamed('b') });
    await flush();
    expect(result.current.map((i) => i.code)).toEqual(['new']);
    await act(async () => answerOld([lint('old')]));
    expect(result.current.map((i) => i.code)).toEqual(['new']);
  });

  it('shows no backend issues when the lint call fails', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    vi.mocked(lintFlow).mockResolvedValueOnce([lint('first')]).mockRejectedValueOnce('boom');
    const { result, rerender } = renderHook(({ flow }) => useBackendFlowLints('demo', flow), {
      initialProps: { flow: flowNamed('a') },
    });
    await flush();
    rerender({ flow: flowNamed('b') });
    await flush();
    expect(result.current).toEqual([]);
    expect(warn).toHaveBeenCalled();
    warn.mockRestore();
  });

  it('does nothing without a collection or a flow', async () => {
    renderHook(() => useBackendFlowLints(null, flowNamed('a')));
    renderHook(() => useBackendFlowLints('demo', null));
    await flush();
    expect(lintFlow).not.toHaveBeenCalled();
  });

  it('cancels a pending lint on unmount', async () => {
    vi.mocked(lintFlow).mockResolvedValue([]);
    const { unmount } = renderHook(() => useBackendFlowLints('demo', flowNamed('a')));
    unmount();
    await flush();
    expect(lintFlow).not.toHaveBeenCalled();
  });
});
