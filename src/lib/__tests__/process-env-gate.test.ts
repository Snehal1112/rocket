import { QueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { warnIfProcessEnvWithheld } from '@/lib/process-env-gate';
import { setQueryClient } from '@/lib/query-client';
import * as tauriApi from '@/lib/tauri-api';
import { useConsoleStore } from '@/stores/console-store';
import { usePaneStore } from '@/stores/pane-store';
import { makeTrust } from '@/test/trust-fixtures';

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
});
