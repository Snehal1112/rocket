import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { environmentKeys, useProcessEnvVars } from '@/lib/queries/environment-queries';
import * as tauriApi from '@/lib/tauri-api';

vi.mock('@/lib/tauri-api', async () => ({
  ...(await vi.importActual<typeof tauriApi>('@/lib/tauri-api')),
  getProcessEnvVars: vi.fn(),
}));

let queryClient: QueryClient;

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

describe('useProcessEnvVars', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  });

  it('does not call the backend when disabled', async () => {
    const { result } = renderHook(() => useProcessEnvVars('clone', false), { wrapper });
    expect(result.current.fetchStatus).toBe('idle');
    expect(tauriApi.getProcessEnvVars).not.toHaveBeenCalled();
  });

  it('returns no variables when disabled even with a warm cache', async () => {
    queryClient.setQueryData(environmentKeys.process(null), {
      vars: { HOME: '/home/u' },
      dotEnvKeys: [],
    });
    const { result } = renderHook(() => useProcessEnvVars(null, false), { wrapper });
    expect(result.current.data).toEqual({});
  });
});
