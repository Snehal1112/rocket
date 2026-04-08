import { act, renderHook } from '@testing-library/react';
import { toast } from 'sonner';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useSaveButton } from './use-save-button';

vi.mock('sonner', () => ({ toast: { error: vi.fn() } }));

describe('useSaveButton', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('starts in idle state', () => {
    const { result } = renderHook(() => useSaveButton(async () => {}));
    expect(result.current.state).toBe('idle');
  });

  it('transitions idle → saving → success → idle on success', async () => {
    const fn = vi.fn().mockResolvedValue(undefined);
    const { result } = renderHook(() => useSaveButton(fn));

    await act(async () => {
      void result.current.trigger();
    });

    expect(result.current.state).toBe('success');

    act(() => vi.advanceTimersByTime(2000));
    expect(result.current.state).toBe('idle');
  });

  it('transitions idle → saving → idle on error and calls toast.error', async () => {
    const fn = vi.fn().mockRejectedValue(new Error('boom'));
    const { result } = renderHook(() => useSaveButton(fn, 'Custom error'));

    await act(async () => {
      void result.current.trigger();
    });

    expect(result.current.state).toBe('idle');
    expect(toast.error).toHaveBeenCalledWith('Custom error');
  });

  it('ignores trigger calls when not idle', async () => {
    let resolve!: () => void;
    const fn = vi.fn().mockImplementation(() => new Promise<void>((r) => { resolve = r; }));
    const { result } = renderHook(() => useSaveButton(fn));

    act(() => { void result.current.trigger(); });
    expect(result.current.state).toBe('saving');

    act(() => { void result.current.trigger(); });
    expect(fn).toHaveBeenCalledTimes(1);

    await act(async () => { resolve(); });
  });
});
