import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { listFlows } from '@/lib/tauri-api';
import { flowKeys, useFlows } from '../flow-queries';

vi.mock('@/lib/tauri-api', () => ({ listFlows: vi.fn() }));

function wrapperFor(client: QueryClient) {
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
}

describe('useFlows', () => {
  beforeEach(() => {
    vi.mocked(listFlows).mockReset();
  });

  it('lists the flows of a collection under its own key', async () => {
    vi.mocked(listFlows).mockResolvedValue(['Login', 'Sync']);
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const { result } = renderHook(() => useFlows('demo'), { wrapper: wrapperFor(client) });
    await waitFor(() => expect(result.current.data).toEqual(['Login', 'Sync']));
    expect(listFlows).toHaveBeenCalledWith('demo');
    expect(client.getQueryData(flowKeys.collection('demo'))).toEqual(['Login', 'Sync']);
  });

  it('does not fetch without a collection or while disabled', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    renderHook(() => useFlows(null), { wrapper: wrapperFor(client) });
    renderHook(() => useFlows('demo', false), { wrapper: wrapperFor(client) });
    await Promise.resolve();
    expect(listFlows).not.toHaveBeenCalled();
  });

  it('refetches after the collection key is invalidated', async () => {
    vi.mocked(listFlows).mockResolvedValueOnce(['Login']).mockResolvedValue(['Login', 'New']);
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const { result } = renderHook(() => useFlows('demo'), { wrapper: wrapperFor(client) });
    await waitFor(() => expect(result.current.data).toEqual(['Login']));
    await client.invalidateQueries({ queryKey: flowKeys.collection('demo') });
    await waitFor(() => expect(result.current.data).toEqual(['Login', 'New']));
  });
});
