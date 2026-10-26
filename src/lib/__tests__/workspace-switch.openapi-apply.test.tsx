import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { useApplySync, useIsApplying, useLastApply } from '@/lib/queries/openapi-sync-queries';
import * as tauriApi from '@/lib/tauri-api';
import { clearWorkspaceScopedCaches } from '@/lib/workspace-switch';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, applyOpenApiSync: vi.fn() };
});

const selection = { items: [], skips: [], variables: [], renameLinks: [] };
const report = {
  applied: [],
  stale: [],
  notRewritable: [],
  deleted: [],
  detached: [],
  skipped: 0,
  remaining: 0,
};

function makeWrapper() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return { client, wrapper };
}

describe('clearWorkspaceScopedCaches and the apply state', () => {
  it('forgets the last apply outcome of the workspace that was left', async () => {
    vi.mocked(tauriApi.applyOpenApiSync).mockResolvedValue(report);
    const { client, wrapper } = makeWrapper();
    const runner = renderHook(() => useApplySync('ws-last'), { wrapper });
    const watch = renderHook(() => useLastApply('ws-last'), { wrapper });
    await runner.result.current.mutateAsync(selection);
    await waitFor(() => expect(watch.result.current.last?.report).toEqual(report));
    act(() => clearWorkspaceScopedCaches(client));
    expect(watch.result.current.last).toBeNull();
  });

  it('forgets an apply that was still running, so the new workspace is not blocked', async () => {
    vi.mocked(tauriApi.applyOpenApiSync).mockReturnValue(new Promise(() => undefined));
    const { client, wrapper } = makeWrapper();
    const runner = renderHook(() => useApplySync('ws-busy'), { wrapper });
    const watch = renderHook(() => useIsApplying('ws-busy'), { wrapper });
    runner.result.current.mutate(selection);
    await waitFor(() => expect(watch.result.current).toBe(true));
    act(() => clearWorkspaceScopedCaches(client));
    expect(watch.result.current).toBe(false);
  });
});
