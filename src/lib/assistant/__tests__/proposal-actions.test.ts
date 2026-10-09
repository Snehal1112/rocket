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
});
