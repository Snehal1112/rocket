import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getOpenApiSyncPlan, getOpenApiSyncStatus, onOpenApiSyncChecked } from '../tauri-api';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));

describe('OpenAPI sync API', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(listen).mockReset();
  });

  it('getOpenApiSyncStatus invokes the status command', async () => {
    vi.mocked(invoke).mockResolvedValue(null);
    await expect(getOpenApiSyncStatus('Orders')).resolves.toBeNull();
    expect(invoke).toHaveBeenCalledWith('openapi_sync_status', { collection: 'Orders' });
  });

  it('getOpenApiSyncPlan invokes the plan command', async () => {
    const plan = { pending: false, items: [] };
    vi.mocked(invoke).mockResolvedValue(plan);
    await expect(getOpenApiSyncPlan('Orders')).resolves.toEqual(plan);
    expect(invoke).toHaveBeenCalledWith('openapi_sync_plan', { collection: 'Orders' });
  });

  it('onOpenApiSyncChecked listens to the checked event and passes the payload', async () => {
    vi.mocked(listen).mockResolvedValue(() => undefined);
    const handler = vi.fn();
    await onOpenApiSyncChecked(handler);
    expect(listen).toHaveBeenCalledWith('openapi-sync-checked', expect.any(Function));
    const cb = vi.mocked(listen).mock.calls[0][1] as (e: { payload: unknown }) => void;
    cb({ payload: { collection: 'Orders', status: 'changed' } });
    expect(handler).toHaveBeenCalledWith({ collection: 'Orders', status: 'changed' });
  });
});
