import { QueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  CLOSED_PROCESS_ENV,
  toProcessEnvScopeInput,
  warnIfProcessEnvWithheld,
} from '@/lib/process-env-gate';
import { setQueryClient } from '@/lib/query-client';
import * as tauriApi from '@/lib/tauri-api';
import { useConsoleStore } from '@/stores/console-store';
import { useEnvStore } from '@/stores/env-store';
import { usePaneStore } from '@/stores/pane-store';
import { makeTrust } from '@/test/trust-fixtures';

describe('toProcessEnvScopeInput', () => {
  it('maps a scratch, a closed and a collection scope', () => {
    useEnvStore.setState({ activeCollection: 'api', activeEnvId: 'dev' });
    expect(toProcessEnvScopeInput(undefined)).toEqual({ kind: 'scratch' });
    expect(toProcessEnvScopeInput(CLOSED_PROCESS_ENV)).toEqual({ kind: 'closed' });
    expect(toProcessEnvScopeInput('api')).toEqual({
      kind: 'collection',
      name: 'api',
      environmentName: 'dev',
    });
  });

  it('toProcessEnvScopeInput omits the environment for a non-active collection', () => {
    useEnvStore.setState({ activeCollection: 'api', activeEnvId: 'dev' });
    expect(toProcessEnvScopeInput('clone')).toEqual({ kind: 'collection', name: 'clone' });
  });

  it('toProcessEnvScopeInput omits the environment when none is active', () => {
    useEnvStore.setState({ activeCollection: 'api', activeEnvId: null });
    expect(toProcessEnvScopeInput('api')).toEqual({ kind: 'collection', name: 'api' });
  });
});

vi.mock('sonner', () => ({ toast: { warning: vi.fn() } }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, getCollectionTrust: vi.fn() };
});

describe('warnIfProcessEnvWithheld', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setQueryClient(new QueryClient({ defaultOptions: { queries: { retry: false } } }));
    useConsoleStore.getState().clearEntries();
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(makeTrust());
  });

  it('warns in the console and with a toast when the placeholder is withheld', async () => {
    await warnIfProcessEnvWithheld('c', [{ headers: [{ value: '{{process.env.HOME}}' }] }], 'Get');
    expect(toast.warning).toHaveBeenCalledTimes(1);
    const entries = useConsoleStore.getState().entries;
    expect(entries).toHaveLength(1);
    expect(entries[0]).toMatchObject({ kind: 'script', level: 'warn', requestName: 'Get' });
  });

  it('the toast action opens the collection overview', async () => {
    const open = vi.spyOn(usePaneStore.getState(), 'openCollectionTab').mockReturnValue(true);
    await warnIfProcessEnvWithheld('c', ['{{process.env.HOME}}']);
    const options = vi.mocked(toast.warning).mock.calls[0]?.[1] as unknown as {
      action: { onClick: () => void };
    };
    options.action.onClick();
    expect(open).toHaveBeenCalledWith('c', 'overview');
  });

  it('stays silent when host environment access is granted', async () => {
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(
      makeTrust({ processEnv: { granted: true } }),
    );
    await warnIfProcessEnvWithheld('c', ['{{process.env.HOME}}']);
    expect(toast.warning).not.toHaveBeenCalled();
  });

  it('stays silent without a placeholder, without a collection, or on a trust error', async () => {
    await warnIfProcessEnvWithheld('c', ['plain text', { a: 1 }, null]);
    await warnIfProcessEnvWithheld(undefined, ['{{process.env.HOME}}']);
    expect(tauriApi.getCollectionTrust).not.toHaveBeenCalled();

    vi.mocked(tauriApi.getCollectionTrust).mockRejectedValue(new Error('boom'));
    await warnIfProcessEnvWithheld('c', ['{{process.env.HOME}}']);
    expect(toast.warning).not.toHaveBeenCalled();
  });

  const CLOSED_MESSAGE =
    'Host environment variables are not available for a request reopened from History without a collection, so {{process.env.NAME}} was sent unresolved.';

  it('warns for a closed scope when the request uses process.env', async () => {
    await warnIfProcessEnvWithheld(
      CLOSED_PROCESS_ENV,
      [{ url: 'https://e.example/{{process.env.HOME}}' }],
      'Get',
    );
    expect(toast.warning).toHaveBeenCalledTimes(1);
    const entries = useConsoleStore.getState().entries;
    expect(entries).toHaveLength(1);
    expect(entries[0]).toMatchObject({ kind: 'script', level: 'warn', requestName: 'Get' });
    expect(entries[0]).toMatchObject({ message: CLOSED_MESSAGE });
    expect(vi.mocked(toast.warning).mock.calls[0]?.[0]).toBe(CLOSED_MESSAGE);
    const options = vi.mocked(toast.warning).mock.calls[0]?.[1] as { action?: unknown } | undefined;
    expect(options?.action).toBeUndefined();
    expect(tauriApi.getCollectionTrust).not.toHaveBeenCalled();
  });

  it('stays silent for a closed scope without the placeholder', async () => {
    await warnIfProcessEnvWithheld(CLOSED_PROCESS_ENV, [{ url: 'https://e.example/' }], 'Get');
    expect(toast.warning).not.toHaveBeenCalled();
    expect(useConsoleStore.getState().entries).toHaveLength(0);
  });
});
