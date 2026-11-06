import { describe, expect, it, vi } from 'vitest';
import { createListenerScope } from '@/lib/listen-scope';
import { createDeferred } from '@/test/deferred';

const settle = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

describe('createListenerScope', () => {
  it('keeps listeners until dispose, then calls each one once', async () => {
    const scope = createListenerScope();
    const first = vi.fn();
    const second = vi.fn();
    scope.keep(Promise.resolve(first));
    scope.keep(Promise.resolve(second));
    await settle();
    expect(first).not.toHaveBeenCalled();
    scope.dispose();
    scope.dispose();
    expect(first).toHaveBeenCalledTimes(1);
    expect(second).toHaveBeenCalledTimes(1);
  });

  it('reports whether it was disposed', () => {
    const scope = createListenerScope();
    expect(scope.isDisposed()).toBe(false);
    scope.dispose();
    expect(scope.isDisposed()).toBe(true);
  });

  it('unlistens a listener that registers after dispose, at once', async () => {
    const scope = createListenerScope();
    const late = createDeferred<() => void>();
    const unlisten = vi.fn();
    scope.keep(late.promise);
    scope.dispose();
    expect(unlisten).not.toHaveBeenCalled();
    late.resolve(unlisten);
    await settle();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it('swallows a failed registration and keeps the others', async () => {
    const scope = createListenerScope();
    const ok = vi.fn();
    scope.keep(Promise.reject(new Error('no event bridge')));
    scope.keep(Promise.resolve(ok));
    await settle();
    expect(() => scope.dispose()).not.toThrow();
    expect(ok).toHaveBeenCalledTimes(1);
  });
});
