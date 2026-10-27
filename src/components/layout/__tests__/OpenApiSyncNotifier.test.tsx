import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, waitFor } from '@testing-library/react';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { OpenApiSyncNotifier } from '../OpenApiSyncNotifier';
import { clearSessionNotified } from '../notifyUpdates';

vi.mock('sonner', () => ({ toast: { info: vi.fn() } }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    onOpenApiSyncChecked: vi.fn(),
    onOpenApiSyncApplied: vi.fn(),
    getOpenApiSyncStatus: vi.fn(),
    getOpenApiSyncBadges: vi.fn(),
  };
});

type Handler = (e: { collection: string; status: string }) => void;
let handler: Handler = () => undefined;

function status(over: Partial<tauriApi.OpenApiSyncStatus> = {}): tauriApi.OpenApiSyncStatus {
  return {
    location: 'https://example.test/openapi.json',
    interval: 'daily',
    lastCheckedAt: null,
    lastStatus: null,
    hasPending: true,
    consent: 'granted',
    pendingId: 'abc123def456',
    ...over,
  } as tauriApi.OpenApiSyncStatus;
}

function mount() {
  render(
    <QueryClientProvider client={new QueryClient()}>
      <OpenApiSyncNotifier />
    </QueryClientProvider>,
  );
}

const fire = (collection = 'Orders', s = 'changed') => handler({ collection, status: s });

describe('OpenApiSyncNotifier toast', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    clearSessionNotified();
    usePaneStore.getState().reset();
    vi.mocked(tauriApi.onOpenApiSyncChecked).mockImplementation(async (h) => {
      handler = h;
      return () => undefined;
    });
    vi.mocked(tauriApi.onOpenApiSyncApplied).mockResolvedValue(() => undefined);
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(status());
    vi.mocked(tauriApi.getOpenApiSyncBadges).mockResolvedValue([
      { collection: 'Orders', waiting: 5 },
    ]);
  });

  it('shows_a_toast_for_a_changed_check', async () => {
    mount();
    fire();
    await waitFor(() => expect(toast.info).toHaveBeenCalledTimes(1));
    const [text, opts] = vi.mocked(toast.info).mock.calls[0];
    expect(text).toBe('Orders has 5 spec updates');
    expect(opts?.action).toMatchObject({ label: 'Review' });
    expect(tauriApi.getOpenApiSyncStatus).toHaveBeenCalledWith('Orders', false);
  });

  it('uses_the_singular_for_one_update', async () => {
    vi.mocked(tauriApi.getOpenApiSyncBadges).mockResolvedValue([
      { collection: 'Orders', waiting: 1 },
    ]);
    mount();
    fire();
    await waitFor(() => expect(toast.info).toHaveBeenCalledWith('Orders has 1 spec update', expect.anything()));
  });

  it('does_not_toast_twice_for_the_same_pending_id', async () => {
    mount();
    fire();
    fire();
    await waitFor(() => expect(toast.info).toHaveBeenCalledTimes(1));
    fire();
    await new Promise((r) => setTimeout(r, 30));
    expect(toast.info).toHaveBeenCalledTimes(1);
  });

  it('toasts_again_for_a_new_pending_id', async () => {
    mount();
    fire();
    await waitFor(() => expect(toast.info).toHaveBeenCalledTimes(1));
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(status({ pendingId: 'ffffffffffff' }));
    fire();
    await waitFor(() => expect(toast.info).toHaveBeenCalledTimes(2));
  });

  it('does_not_toast_for_unchanged_or_failed_events', async () => {
    mount();
    fire('Orders', 'unchanged');
    fire('Orders', 'failed');
    await new Promise((r) => setTimeout(r, 30));
    expect(toast.info).not.toHaveBeenCalled();
    expect(tauriApi.getOpenApiSyncStatus).not.toHaveBeenCalled();
  });

  it('does_not_toast_without_consent', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(status({ consent: 'missing' }));
    mount();
    fire();
    await waitFor(() => expect(tauriApi.getOpenApiSyncStatus).toHaveBeenCalled());
    await new Promise((r) => setTimeout(r, 30));
    expect(toast.info).not.toHaveBeenCalled();
  });

  it('does_not_toast_when_the_tab_is_active', async () => {
    usePaneStore.getState().openOpenApiSyncTab('Orders');
    mount();
    fire();
    await waitFor(() => expect(tauriApi.getOpenApiSyncStatus).toHaveBeenCalled());
    await new Promise((r) => setTimeout(r, 30));
    expect(toast.info).not.toHaveBeenCalled();
  });

  it('still_toasts_once_when_storage_throws', async () => {
    const spy = vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    mount();
    fire();
    await waitFor(() => expect(toast.info).toHaveBeenCalledTimes(1));
    fire();
    await new Promise((r) => setTimeout(r, 30));
    expect(toast.info).toHaveBeenCalledTimes(1);
    spy.mockRestore();
    vi.restoreAllMocks();
  });

  it('retries_after_a_failed_badge_lookup', async () => {
    vi.mocked(tauriApi.getOpenApiSyncBadges).mockRejectedValueOnce(new Error('boom'));
    mount();
    fire();
    await waitFor(() => expect(tauriApi.getOpenApiSyncBadges).toHaveBeenCalledTimes(1));
    await new Promise((r) => setTimeout(r, 30));
    expect(toast.info).not.toHaveBeenCalled();
    fire();
    await waitFor(() => expect(toast.info).toHaveBeenCalledTimes(1));
  });

  it('a_zero_count_does_not_consume_the_id', async () => {
    vi.mocked(tauriApi.getOpenApiSyncBadges).mockResolvedValueOnce([
      { collection: 'Orders', waiting: 0 },
    ]);
    mount();
    fire();
    await waitFor(() => expect(tauriApi.getOpenApiSyncBadges).toHaveBeenCalledTimes(1));
    await new Promise((r) => setTimeout(r, 30));
    expect(toast.info).not.toHaveBeenCalled();
    fire();
    await waitFor(() => expect(toast.info).toHaveBeenCalledTimes(1));
  });

  it('a_throwing_toast_does_not_consume_the_id', async () => {
    vi.mocked(toast.info).mockImplementationOnce(() => {
      throw new Error('toast failed');
    });
    mount();
    fire();
    await waitFor(() => expect(toast.info).toHaveBeenCalledTimes(1));
    await new Promise((r) => setTimeout(r, 30));
    fire();
    await waitFor(() => expect(toast.info).toHaveBeenCalledTimes(2));
  });

  it('the_review_action_opens_the_tab', async () => {
    mount();
    fire();
    await waitFor(() => expect(toast.info).toHaveBeenCalled());
    const action = vi.mocked(toast.info).mock.calls[0][1]?.action as unknown as {
      onClick: () => void;
    };
    action.onClick();
    expect(usePaneStore.getState().root).toBeDefined();
    const root = usePaneStore.getState().root;
    expect(root.type === 'leaf' && root.activeTabId).toBe('openapi-sync:Orders');
  });
});
