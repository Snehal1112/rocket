import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    listAgentConfigs: vi.fn(),
    saveAgentConfig: vi.fn(),
    deleteAgentConfig: vi.fn(),
    testAgentConfig: vi.fn(),
  };
});

let queryClient: QueryClient;

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

const sampleConfig: tauriApi.AgentConfig = {
  id: 'agent-1',
  label: 'Claude Agent',
  command: 'claude-agent-acp',
  args: ['--stdio'],
  workingDir: undefined,
  credentialEnvVar: 'ANTHROPIC_API_KEY',
  vaultConnectionId: 'conn-1',
  vaultName: 'prod-vault',
  vaultSecretId: 'secret-id-1',
  vaultSecretName: 'anthropic-api-key',
};

describe('agent config queries', () => {
  beforeEach(() => {
    // Mock call counts persist across tests in this file (no global
    // clearMocks) — clear them so each test's toHaveBeenCalledTimes
    // assertion counts only its own calls.
    vi.clearAllMocks();
    // staleTime mirrors the app's production QueryClient so this test
    // reflects real refetch-on-mutation-invalidation behavior rather than
    // the library's staleTime: 0 default, which triggers an extra
    // refetch-on-mount unrelated to the invalidation this test checks for
    // (see collection-queries.test.tsx for the same reasoning).
    queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false, staleTime: 30_000 } },
    });
    vi.mocked(tauriApi.listAgentConfigs).mockResolvedValue([sampleConfig]);
    vi.mocked(tauriApi.saveAgentConfig).mockResolvedValue(undefined);
    vi.mocked(tauriApi.deleteAgentConfig).mockResolvedValue(undefined);
    vi.mocked(tauriApi.testAgentConfig).mockResolvedValue(undefined);
  });

  it('useAgentConfigs fetches the list', async () => {
    const { useAgentConfigs } = await import('../agent-config-queries');
    const { result } = renderHook(() => useAgentConfigs(), { wrapper });
    await waitFor(() => expect(result.current.data).toEqual([sampleConfig]));
  });

  it('useSaveAgentConfig calls saveAgentConfig and invalidates the list', async () => {
    const { useAgentConfigs, useSaveAgentConfig } = await import('../agent-config-queries');
    const { result: list } = renderHook(() => useAgentConfigs(), { wrapper });
    await waitFor(() => expect(list.current.data).toEqual([sampleConfig]));

    const { result: save } = renderHook(() => useSaveAgentConfig(), { wrapper });
    await save.current.mutateAsync(sampleConfig);

    expect(tauriApi.saveAgentConfig).toHaveBeenCalledWith(sampleConfig);
    expect(tauriApi.listAgentConfigs).toHaveBeenCalledTimes(2);
  });

  it('useDeleteAgentConfig calls deleteAgentConfig with the id', async () => {
    const { useDeleteAgentConfig } = await import('../agent-config-queries');
    const { result } = renderHook(() => useDeleteAgentConfig(), { wrapper });
    await result.current.mutateAsync('agent-1');
    expect(tauriApi.deleteAgentConfig).toHaveBeenCalledWith('agent-1');
  });

  it('useTestAgentConfig calls testAgentConfig with the id', async () => {
    const { useTestAgentConfig } = await import('../agent-config-queries');
    const { result } = renderHook(() => useTestAgentConfig(), { wrapper });
    await result.current.mutateAsync('agent-1');
    expect(tauriApi.testAgentConfig).toHaveBeenCalledWith('agent-1');
  });
});
