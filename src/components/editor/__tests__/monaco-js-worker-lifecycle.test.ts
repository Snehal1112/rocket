import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const setCompilerOptions = vi.fn();
const getCompilerOptions = vi.fn(() => ({ target: 99 }));

vi.mock('monaco-editor', () => ({
  typescript: {
    javascriptDefaults: { setCompilerOptions, getCompilerOptions },
  },
}));

describe('monaco-js-worker-lifecycle', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    // Reset the module registry so each test gets a fresh ref count instead
    // of depending on prior tests leaving it balanced back to 0.
    vi.resetModules();
    setCompilerOptions.mockClear();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('does not tear down the worker while another reference is still held', async () => {
    const { acquireJsWorker, releaseJsWorker } = await import('../monaco-js-worker-lifecycle');
    acquireJsWorker();
    acquireJsWorker();
    releaseJsWorker();
    vi.advanceTimersByTime(0);

    expect(setCompilerOptions).not.toHaveBeenCalled();
  });

  it('tears down the worker once the last reference releases', async () => {
    const { acquireJsWorker, releaseJsWorker } = await import('../monaco-js-worker-lifecycle');
    acquireJsWorker();
    releaseJsWorker();
    vi.advanceTimersByTime(0);

    expect(setCompilerOptions).toHaveBeenCalledTimes(1);
    expect(setCompilerOptions).toHaveBeenCalledWith({ target: 99 });
  });

  it('is a no-op to release when the count is already at zero', async () => {
    const { releaseJsWorker } = await import('../monaco-js-worker-lifecycle');
    releaseJsWorker();
    vi.advanceTimersByTime(0);

    expect(setCompilerOptions).not.toHaveBeenCalled();
  });

  it('does not tear down across a React StrictMode dev double-invoke while still mounted, but does on a later real unmount', async () => {
    const { acquireJsWorker, releaseJsWorker } = await import('../monaco-js-worker-lifecycle');

    // StrictMode's dev double-invoke is mount -> cleanup -> mount, then the
    // component stays mounted (no second unmount happens as part of the
    // simulation itself).
    acquireJsWorker();
    releaseJsWorker();
    acquireJsWorker();
    vi.advanceTimersByTime(0);

    expect(setCompilerOptions).not.toHaveBeenCalled();

    // A later, real unmount should still release the worker exactly once.
    releaseJsWorker();
    vi.advanceTimersByTime(0);

    expect(setCompilerOptions).toHaveBeenCalledTimes(1);
  });

  it('cancels a pending teardown if a new acquire arrives during the deferral window', async () => {
    const { acquireJsWorker, releaseJsWorker } = await import('../monaco-js-worker-lifecycle');
    acquireJsWorker();
    releaseJsWorker(); // schedules a deferred teardown
    acquireJsWorker(); // arrives before the deferred teardown fires
    vi.advanceTimersByTime(0);

    expect(setCompilerOptions).not.toHaveBeenCalled();
  });
});
