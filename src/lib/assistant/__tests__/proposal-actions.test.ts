import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createDefaultRequest } from '@/lib/pane-utils';
import * as api from '@/lib/tauri-api';
import type { AgentProposal } from '@/lib/tauri-api';
import { useAssistantStore } from '@/stores/assistant-store';
import { usePaneStore } from '@/stores/pane-store';
import { makeProposal, makeRequest } from '@/test/assistant-fixtures';
import { isRequestTab, type RequestTab } from '@/types/pane-types';
import { acceptProposal, hasDirtyAffectedTab, rejectProposal } from '../proposal-actions';

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  acceptAgentProposal: vi.fn(),
  rejectAgentProposal: vi.fn(),
  getRequest: vi.fn(),
}));

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));

function requestTab(path: string, overrides: Partial<RequestTab> = {}): RequestTab {
  return {
    id: `tab:${path}`,
    title: path,
    tabType: 'request',
    request: { ...createDefaultRequest(), testsScript: 'old();' },
    response: null,
    isDirty: false,
    source: { collection: 'orders', path },
    ...overrides,
  };
}

function firstTab(): RequestTab | undefined {
  const { root } = usePaneStore.getState();
  const tab = root.type === 'leaf' ? root.tabs[0] : undefined;
  return tab && isRequestTab(tab) ? tab : undefined;
}

describe('proposal actions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    useAssistantStore.getState().reset();
    const token = useAssistantStore.getState().beginSession('agent-1', 'edit');
    useAssistantStore.getState().activateSession(token, 's1', []);
  });

  it('accept stores the result and reloads a clean open tab of that request', async () => {
    usePaneStore.getState().openTab(requestTab('get.yml'));
    const proposal = makeProposal();
    useAssistantStore.getState().upsertProposal(proposal);
    vi.mocked(api.acceptAgentProposal).mockResolvedValue({ ...proposal, status: 'accepted' });
    vi.mocked(api.getRequest).mockResolvedValue(makeRequest({ tests: 'new();' }));

    await acceptProposal(proposal);

    expect(api.acceptAgentProposal).toHaveBeenCalledWith('s1', 'p1');
    expect(useAssistantStore.getState().proposals[0].status).toBe('accepted');
    expect(firstTab()?.request.testsScript).toBe('new();');
    expect(firstTab()?.isDirty).toBe(false);
  });

  it('reloads nothing when the change was not applied', async () => {
    usePaneStore.getState().openTab(requestTab('get.yml'));
    const proposal = makeProposal();
    useAssistantStore.getState().upsertProposal(proposal);
    vi.mocked(api.acceptAgentProposal).mockResolvedValue({ ...proposal, status: 'stale' });

    await acceptProposal(proposal);

    expect(useAssistantStore.getState().proposals[0].status).toBe('stale');
    expect(api.getRequest).not.toHaveBeenCalled();
    expect(firstTab()?.request.testsScript).toBe('old();');
  });

  it('reject stores the result', async () => {
    const proposal = makeProposal();
    useAssistantStore.getState().upsertProposal(proposal);
    vi.mocked(api.rejectAgentProposal).mockResolvedValue({ ...proposal, status: 'rejected' });

    await rejectProposal(proposal);

    expect(api.rejectAgentProposal).toHaveBeenCalledWith('s1', 'p1');
    expect(useAssistantStore.getState().proposals[0].status).toBe('rejected');
  });

  it('finds unsaved edits in an open tab of the changed request', () => {
    usePaneStore.getState().openTab(requestTab('get.yml', { isDirty: true }));
    expect(hasDirtyAffectedTab(usePaneStore.getState(), makeProposal().change)).toBe(true);
  });

  it('finds unsaved edits in a tab parked after a collection switch', () => {
    usePaneStore.setState({
      collectionTabState: {
        orders: { tabs: [requestTab('get.yml', { isDirty: true })], activeTabId: 'tab:get.yml' },
      },
    });
    expect(hasDirtyAffectedTab(usePaneStore.getState(), makeProposal().change)).toBe(true);
  });

  it('finds unsaved edits inside a folder that is being moved', () => {
    usePaneStore.getState().openTab(requestTab('users/get.yml', { isDirty: true }));
    const change: AgentProposal['change'] = {
      op: 'moveItem',
      collection: 'orders',
      fromPath: 'users',
      toFolder: 'archive',
    };
    expect(hasDirtyAffectedTab(usePaneStore.getState(), change)).toBe(true);
  });

  it('ignores clean tabs, other requests and other collections', () => {
    usePaneStore.getState().openTab(requestTab('get.yml'));
    usePaneStore.getState().openTab(requestTab('other.yml', { isDirty: true }));
    usePaneStore.getState().openTab(
      requestTab('get.yml', {
        id: 'tab:billing',
        isDirty: true,
        source: { collection: 'billing', path: 'get.yml' },
      }),
    );
    expect(hasDirtyAffectedTab(usePaneStore.getState(), makeProposal().change)).toBe(false);
  });

  it('refreshes a clean tab parked in another collection snapshot', async () => {
    usePaneStore.setState({
      collectionTabState: {
        orders: { tabs: [requestTab('get.yml')], activeTabId: 'tab:get.yml' },
      },
    });
    const proposal = makeProposal();
    useAssistantStore.getState().upsertProposal(proposal);
    vi.mocked(api.acceptAgentProposal).mockResolvedValue({ ...proposal, status: 'accepted' });
    vi.mocked(api.getRequest).mockResolvedValue(makeRequest({ tests: 'new();' }));

    await acceptProposal(proposal);

    const parked = usePaneStore.getState().collectionTabState.orders.tabs[0];
    expect(parked && isRequestTab(parked) && parked.request.testsScript).toBe('new();');
  });

  it('keeps the edits of a tab dirtied during accept and warns', async () => {
    usePaneStore.getState().openTab(requestTab('get.yml'));
    const proposal = makeProposal();
    useAssistantStore.getState().upsertProposal(proposal);
    vi.mocked(api.acceptAgentProposal).mockImplementation(async () => {
      usePaneStore.getState().markDirty('tab:get.yml');
      return { ...proposal, status: 'accepted' };
    });
    vi.mocked(api.getRequest).mockResolvedValue(makeRequest({ tests: 'new();' }));

    const warning = await acceptProposal(proposal);

    expect(warning).toMatch(/edited while the change was applied/);
    expect(firstTab()?.request.testsScript).toBe('old();');
    expect(firstTab()?.isDirty).toBe(true);
  });

  it('retargets live and parked tabs of a moved folder', async () => {
    usePaneStore.getState().openTab(requestTab('users/get.yml'));
    usePaneStore.setState({
      collectionTabState: {
        orders: { tabs: [requestTab('users/list.yml', { id: 'tab:list' })], activeTabId: 'tab:list' },
      },
    });
    const change: AgentProposal['change'] = {
      op: 'moveItem',
      collection: 'orders',
      fromPath: 'users',
      toFolder: 'archive',
    };
    const proposal = makeProposal({ change });
    useAssistantStore.getState().upsertProposal(proposal);
    vi.mocked(api.acceptAgentProposal).mockResolvedValue({ ...proposal, status: 'accepted' });

    await acceptProposal(proposal);

    expect(firstTab()?.source?.path).toBe('archive/users/get.yml');
    const parked = usePaneStore.getState().collectionTabState.orders.tabs[0];
    expect(parked?.source?.path).toBe('archive/users/list.yml');
  });

  it('retargets tabs of a renamed folder and keeps the path of a renamed request', async () => {
    usePaneStore.getState().openTab(requestTab('users/get.yml'));
    const folder = makeProposal({
      change: { op: 'renameItem', collection: 'orders', path: 'users', newName: 'people' },
    });
    useAssistantStore.getState().upsertProposal(folder);
    vi.mocked(api.acceptAgentProposal).mockResolvedValue({ ...folder, status: 'accepted' });
    vi.mocked(api.getRequest).mockRejectedValue(new Error('not a request'));
    await acceptProposal(folder);
    expect(firstTab()?.source?.path).toBe('people/get.yml');

    const request = makeProposal({
      id: 'p2',
      change: { op: 'renameItem', collection: 'orders', path: 'people/get.yml', newName: 'Get one' },
    });
    useAssistantStore.getState().upsertProposal(request);
    vi.mocked(api.acceptAgentProposal).mockResolvedValue({ ...request, status: 'accepted' });
    vi.mocked(api.getRequest).mockResolvedValue(makeRequest({ name: 'Get one' }));
    await acceptProposal(request);
    expect(firstTab()?.source?.path).toBe('people/get.yml');
    expect(firstTab()?.title).toBe('Get one');
  });
});
