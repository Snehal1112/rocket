import { renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useConsoleStore } from '@/stores/console-store';
import { useBackendLogs } from '../useBackendLogs';

type Handler = (event: { payload: unknown }) => void;

const listenMock = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/event', () => ({ listen: listenMock }));

const payload = {
  timestamp: '2026-10-02T10:00:00Z',
  level: 'WARN',
  target: 't',
  message: 'boom',
  fields: {},
  spanFields: {},
};

describe('useBackendLogs', () => {
  beforeEach(() => {
    listenMock.mockReset();
    useConsoleStore.getState().clearEntries();
  });

  it('adds backend-log events to the store and unlistens on unmount', async () => {
    const unlisten = vi.fn();
    let handler: Handler = () => undefined;
    listenMock.mockImplementation((_name: string, h: Handler) => {
      handler = h;
      return Promise.resolve(unlisten);
    });
    const { unmount } = renderHook(() => useBackendLogs());
    expect(listenMock).toHaveBeenCalledWith('backend-log', expect.any(Function));
    await Promise.resolve();
    handler({ payload });
    expect(useConsoleStore.getState().logEntries[0].message).toBe('boom');
    unmount();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it('unlistens when the promise resolves after unmount', async () => {
    const unlisten = vi.fn();
    let resolve: (fn: () => void) => void = () => undefined;
    listenMock.mockReturnValue(
      new Promise<() => void>((r) => {
        resolve = r;
      }),
    );
    const { unmount } = renderHook(() => useBackendLogs());
    unmount();
    expect(unlisten).not.toHaveBeenCalled();
    resolve(unlisten);
    await Promise.resolve();
    await Promise.resolve();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });
});
