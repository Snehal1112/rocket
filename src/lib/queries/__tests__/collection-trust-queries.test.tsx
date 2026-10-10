import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { makeTrust } from '@/test/trust-fixtures';
import { useCollectionTrust, useCollectionTrustEvents } from '../collection-trust-queries';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getCollectionTrust: vi.fn(),
    onCollectionTrustChanged: vi.fn(),
    onCollectionChanged: vi.fn(),
  };
});

describe('collection trust queries', () => {
  let client: QueryClient;
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );

  beforeEach(() => {
    vi.clearAllMocks();
    client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(makeTrust());
    vi.mocked(tauriApi.onCollectionTrustChanged).mockResolvedValue(() => undefined);
    vi.mocked(tauriApi.onCollectionChanged).mockResolvedValue(() => undefined);
  });

  it('does not query without a collection', () => {
    renderHook(() => useCollectionTrust(null), { wrapper });
    expect(tauriApi.getCollectionTrust).not.toHaveBeenCalled();
  });

  it('refetches when the trust or collection events fire', async () => {
    const { result } = renderHook(
      () => {
        useCollectionTrustEvents();
        return useCollectionTrust('c');
      },
      { wrapper },
    );
    await waitFor(() => expect(result.current.data).toBeDefined());
    expect(tauriApi.getCollectionTrust).toHaveBeenCalledTimes(1);

    const onTrust = vi.mocked(tauriApi.onCollectionTrustChanged).mock.calls[0]?.[0];
    const onFile = vi.mocked(tauriApi.onCollectionChanged).mock.calls[0]?.[0];
    onTrust?.({ collection: 'c' });
    await waitFor(() => expect(tauriApi.getCollectionTrust).toHaveBeenCalledTimes(2));
    onFile?.({ type: 'x' });
    await waitFor(() => expect(tauriApi.getCollectionTrust).toHaveBeenCalledTimes(3));
  });
});
