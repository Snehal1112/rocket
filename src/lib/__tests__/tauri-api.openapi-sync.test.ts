import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  applyOpenApiSync,
  checkOpenApiSyncNow,
  clearOpenApiSyncToken,
  getOpenApiSyncPlan,
  getOpenApiSyncStatus,
  grantOpenApiSyncConsent,
  onOpenApiSyncApplied,
  onOpenApiSyncChecked,
  revokeOpenApiSyncConsent,
  setOpenApiSyncInterval,
  setOpenApiSyncToken,
} from '../tauri-api';

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

  it('getOpenApiSyncStatus sends includeToken only when true', async () => {
    vi.mocked(invoke).mockResolvedValue(null);
    await getOpenApiSyncStatus('Orders', false);
    expect(invoke).toHaveBeenLastCalledWith('openapi_sync_status', { collection: 'Orders' });
    await getOpenApiSyncStatus('Orders', true);
    expect(invoke).toHaveBeenLastCalledWith('openapi_sync_status', {
      collection: 'Orders',
      includeToken: true,
    });
  });

  it('getOpenApiSyncPlan sends links only when there are some', async () => {
    vi.mocked(invoke).mockResolvedValue({});
    await getOpenApiSyncPlan('Orders', []);
    expect(invoke).toHaveBeenLastCalledWith('openapi_sync_plan', { collection: 'Orders' });
    await getOpenApiSyncPlan('Orders', [['a', 'b']]);
    expect(invoke).toHaveBeenLastCalledWith('openapi_sync_plan', {
      collection: 'Orders',
      links: [['a', 'b']],
    });
  });

  it('the action wrappers call their commands with the documented args', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await checkOpenApiSyncNow('Orders');
    expect(invoke).toHaveBeenLastCalledWith('openapi_sync_check_now', { collection: 'Orders' });
    await setOpenApiSyncInterval('Orders', 'daily');
    expect(invoke).toHaveBeenLastCalledWith('openapi_sync_set_interval', {
      collection: 'Orders',
      interval: 'daily',
    });
    await setOpenApiSyncToken('Orders', 'tok');
    expect(invoke).toHaveBeenLastCalledWith('openapi_sync_set_token', {
      collection: 'Orders',
      token: 'tok',
    });
    await clearOpenApiSyncToken('Orders');
    expect(invoke).toHaveBeenLastCalledWith('openapi_sync_clear_token', { collection: 'Orders' });
    await grantOpenApiSyncConsent('Orders', 'https://x/y');
    expect(invoke).toHaveBeenLastCalledWith('openapi_sync_grant_consent', {
      collection: 'Orders',
      location: 'https://x/y',
    });
    await revokeOpenApiSyncConsent('Orders');
    expect(invoke).toHaveBeenLastCalledWith('openapi_sync_revoke_consent', {
      collection: 'Orders',
    });
  });

  it('applyOpenApiSync sends the selection with camelCase renameLinks', async () => {
    vi.mocked(invoke).mockResolvedValue({ applied: [] });
    const selection = {
      items: [{ key: 'k', digest: 'd', choices: {} }],
      skips: [{ key: 's', fingerprint: 'f' }],
      variables: [{ name: 'v', digest: 'dv' }],
      renameLinks: [['a', 'b']] as Array<[string, string]>,
    };
    await applyOpenApiSync('Orders', selection);
    expect(invoke).toHaveBeenCalledWith('openapi_sync_apply', { collection: 'Orders', selection });
  });

  it('onOpenApiSyncApplied listens to the applied event', async () => {
    vi.mocked(listen).mockResolvedValue(() => undefined);
    const handler = vi.fn();
    await onOpenApiSyncApplied(handler);
    expect(listen).toHaveBeenCalledWith('openapi-sync-applied', expect.any(Function));
    const cb = vi.mocked(listen).mock.calls[0][1] as (e: { payload: unknown }) => void;
    cb({ payload: { collection: 'Orders', applied: 2, stale: 1 } });
    expect(handler).toHaveBeenCalledWith({ collection: 'Orders', applied: 2, stale: 1 });
  });
});
