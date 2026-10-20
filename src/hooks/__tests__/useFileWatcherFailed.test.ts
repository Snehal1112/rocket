import { renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useFileWatcherFailed } from '../useFileWatcherFailed';

type Handler = (event: { payload: unknown }) => void;

const listenMock = vi.hoisted(() => vi.fn());
const toastError = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/event', () => ({ listen: listenMock }));
vi.mock('sonner', () => ({ toast: { error: toastError } }));

describe('useFileWatcherFailed', () => {
  beforeEach(() => {
    listenMock.mockReset();
    toastError.mockReset();
  });

  it('shows an error toast on file-watcher-failed and unlistens on unmount', async () => {
    const unlisten = vi.fn();
    let handler: Handler = () => undefined;
    listenMock.mockImplementation((_name: string, h: Handler) => {
      handler = h;
      return Promise.resolve(unlisten);
    });
    const { unmount } = renderHook(() => useFileWatcherFailed());
    expect(listenMock).toHaveBeenCalledWith('file-watcher-failed', expect.any(Function));
    await Promise.resolve();
    handler({ payload: { message: '/x: boom' } });
    expect(toastError).toHaveBeenCalledWith(
      'Automatic refresh is off',
      expect.objectContaining({ description: expect.stringContaining('/x: boom') }),
    );
    unmount();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });
});
