import { QueryClient } from '@tanstack/react-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { toast } from 'sonner';
import {
  buildOAuth2VarContext,
  resolveRequestFields,
  sendRequest,
} from '@/lib/execute-request';
import { createDefaultRequest } from '@/lib/pane-utils';
import { setQueryClient } from '@/lib/query-client';
import * as tauriApi from '@/lib/tauri-api';
import { useEnvStore } from '@/stores/env-store';
import { usePaneStore } from '@/stores/pane-store';
import { makeTrust } from '@/test/trust-fixtures';
import type { RequestState, RequestTab } from '@/types/pane-types';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getProcessEnvVars: vi.fn(),
    getCollectionSettings: vi.fn().mockResolvedValue({ headers: [], variables: [] }),
    executeRequest: vi.fn(),
    getCollectionTrust: vi.fn(),
    getFolderChainVariables: vi.fn(),
  };
});
vi.mock('sonner', () => ({ toast: { warning: vi.fn() } }));

const pe = (vars: Record<string, string>) => ({ vars, dotEnvKeys: [] as string[] });

const URL_WITH_ENV = 'https://e.example/{{process.env.HOME}}';

function requestWithUrl(url: string): RequestState {
  return { ...createDefaultRequest(), url };
}

function openTab(tab: Partial<RequestTab> & { id: string }): RequestState {
  const request = requestWithUrl(URL_WITH_ENV);
  usePaneStore.getState().openTab({
    title: 'tab',
    isDirty: false,
    tabType: 'history',
    request,
    response: null,
    ...tab,
  } as RequestTab);
  return request;
}

describe('process env scope', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    useEnvStore.setState({ activeEnvId: null, activeCollection: null });
    setQueryClient(new QueryClient({ defaultOptions: { queries: { retry: false } } }));
  });

  it('asks for the process env of the history collection', async () => {
    vi.mocked(tauriApi.getProcessEnvVars).mockResolvedValue(pe({ HOME: '/home/u' }));
    const ctx = await buildOAuth2VarContext(undefined, undefined, 'clone');
    expect(tauriApi.getProcessEnvVars).toHaveBeenCalledWith('clone', null, null);
    expect(ctx['process.env.HOME']).toBe('/home/u');
    expect(tauriApi.getCollectionSettings).not.toHaveBeenCalled();
  });

  it('gets no host environment and makes no call for a history tab without a collection', async () => {
    vi.mocked(tauriApi.getProcessEnvVars).mockResolvedValue(pe({ HOME: '/home/u' }));
    const request = openTab({ id: 'history-h1', historyCollection: null });
    const resolved = await resolveRequestFields('history-h1', request);
    expect(tauriApi.getProcessEnvVars).not.toHaveBeenCalled();
    expect(resolved.url).toBe(URL_WITH_ENV);
  });

  it('keeps full access for a scratch tab', async () => {
    vi.mocked(tauriApi.getProcessEnvVars).mockResolvedValue(pe({ HOME: '/home/u' }));
    const request = openTab({ id: 'scratch-1', tabType: 'request' });
    useEnvStore.setState({ activeCollection: 'other' });
    await resolveRequestFields('scratch-1', request);
    expect(tauriApi.getProcessEnvVars).toHaveBeenCalledWith(null, null, null);
  });

  it('does not load collection variables or headers for a history tab', async () => {
    vi.mocked(tauriApi.getProcessEnvVars).mockResolvedValue(pe({}));
    const request = openTab({ id: 'history-h2', historyCollection: 'clone' });
    const resolved = await resolveRequestFields('history-h2', request);
    expect(tauriApi.getProcessEnvVars).toHaveBeenCalledWith('clone', null, null);
    expect(tauriApi.getCollectionSettings).not.toHaveBeenCalled();
    expect(tauriApi.getFolderChainVariables).not.toHaveBeenCalled();
    expect(resolved.collection).toBeUndefined();
  });

  it('lets source win over historyCollection', async () => {
    vi.mocked(tauriApi.getProcessEnvVars).mockResolvedValue(pe({}));
    vi.mocked(tauriApi.getFolderChainVariables).mockResolvedValue([]);
    const request = openTab({
      id: 'history-h3',
      source: { collection: 'a', path: 'x.yml' },
      historyCollection: 'b',
    });
    await resolveRequestFields('history-h3', request);
    expect(tauriApi.getProcessEnvVars).toHaveBeenCalledWith('a', null, null);
    expect(vi.mocked(tauriApi.getProcessEnvVars).mock.calls.some((c) => c[0] === 'b')).toBe(false);
  });

  it('treats a missing history tab as closed', async () => {
    vi.mocked(tauriApi.getProcessEnvVars).mockResolvedValue(pe({ HOME: '/home/u' }));
    await resolveRequestFields('history-gone', requestWithUrl(URL_WITH_ENV));
    expect(tauriApi.getProcessEnvVars).not.toHaveBeenCalled();
  });

  it('sends a history tab without a collection with no process env call and the closed warning', async () => {
    vi.mocked(tauriApi.getProcessEnvVars).mockResolvedValue(pe({ HOME: '/home/u' }));
    const request = openTab({ id: 'history-h4', historyCollection: null });
    await sendRequest('history-h4', request);
    expect(tauriApi.getProcessEnvVars).not.toHaveBeenCalled();
    const sent = vi.mocked(tauriApi.executeRequest).mock.calls[0][0];
    expect(sent.url).toContain('{{process.env.HOME}}');
    expect(sent.collection).toBeUndefined();
    expect(toast.warning).toHaveBeenCalledWith(expect.stringContaining('without a collection'));
  });

  it('sends the tab collection scope with the request', async () => {
    vi.mocked(tauriApi.getProcessEnvVars).mockResolvedValue(pe({}));
    vi.mocked(tauriApi.getFolderChainVariables).mockResolvedValue([]);
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(makeTrust());
    const request = openTab({
      id: 't1',
      tabType: 'request',
      source: { collection: 'api', path: 'users/list' },
    });
    await sendRequest('t1', request);
    const input = vi.mocked(tauriApi.executeRequest).mock.lastCall?.[0];
    expect(input?.processEnv).toEqual({ kind: 'collection', name: 'api' });
  });

  it('sends a closed scope for a History tab with no collection', async () => {
    vi.mocked(tauriApi.getProcessEnvVars).mockResolvedValue(pe({}));
    const request = openTab({ id: 'history-h6', historyCollection: null });
    await sendRequest('history-h6', request);
    const input = vi.mocked(tauriApi.executeRequest).mock.lastCall?.[0];
    expect(input?.processEnv).toEqual({ kind: 'closed' });
  });

  it('warns with the history collection on a resend', async () => {
    vi.mocked(tauriApi.getProcessEnvVars).mockResolvedValue(pe({}));
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(makeTrust());
    const request = openTab({ id: 'history-h5', historyCollection: 'clone' });
    await sendRequest('history-h5', request);
    expect(tauriApi.getCollectionTrust).toHaveBeenCalledWith('clone');
    const calls = vi.mocked(tauriApi.getProcessEnvVars).mock.calls;
    expect(calls.length).toBeGreaterThan(0);
    expect(calls.every((c) => c[0] === 'clone')).toBe(true);
  });

  it('asks for the host environment of the collection and gets none when it is withheld', async () => {
    vi.mocked(tauriApi.getProcessEnvVars).mockResolvedValue(pe({}));
    const ctx = await buildOAuth2VarContext('clone');
    expect(tauriApi.getProcessEnvVars).toHaveBeenCalledWith('clone', null, null);
    expect(Object.keys(ctx).some((k) => k.startsWith('process.env.'))).toBe(false);
  });

  it('adds the host environment when the backend returns it', async () => {
    vi.mocked(tauriApi.getProcessEnvVars).mockResolvedValue(pe({ HOME: '/home/u' }));
    const ctx = await buildOAuth2VarContext('mine');
    expect(ctx['process.env.HOME']).toBe('/home/u');
  });

  it('asks the backend on every send, so a withdrawn permission applies at once', async () => {
    vi.mocked(tauriApi.getProcessEnvVars).mockResolvedValueOnce(pe({ HOME: '/home/u' }));
    expect((await buildOAuth2VarContext('mine'))['process.env.HOME']).toBe('/home/u');

    // The backend now returns nothing for the collection.
    vi.mocked(tauriApi.getProcessEnvVars).mockResolvedValue(pe({}));
    const ctx = await buildOAuth2VarContext('mine');
    expect(ctx['process.env.HOME']).toBeUndefined();
    expect(tauriApi.getProcessEnvVars).toHaveBeenCalledTimes(2);
  });

  it('keeps full access for a scratch request and never throws on a failed read', async () => {
    vi.mocked(tauriApi.getProcessEnvVars).mockResolvedValueOnce(pe({ HOME: '/h' }));
    expect((await buildOAuth2VarContext(undefined))['process.env.HOME']).toBe('/h');
    expect(tauriApi.getProcessEnvVars).toHaveBeenCalledWith(null, null, null);

    vi.mocked(tauriApi.getProcessEnvVars).mockRejectedValue(new Error('boom'));
    const ctx = await buildOAuth2VarContext('other');
    expect(Object.keys(ctx).some((k) => k.startsWith('process.env.'))).toBe(false);
  });
});
