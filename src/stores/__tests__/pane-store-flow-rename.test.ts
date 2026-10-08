import { beforeEach, describe, expect, it, vi } from 'vitest';
import { flowAuthKey } from '@/lib/flow-auth';
import { collectAllTabs } from '@/lib/pane-utils';
import { type AuthState, type FlowTab, isFlowTab } from '@/types/pane-types';
import { useFlowAuthStore } from '../flow-auth-store';
import { getFlow } from '@/lib/tauri-api';
import { usePaneStore } from '../pane-store';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getFlow: vi.fn(), endAgentSession: vi.fn() };
});

const node = {
  id: 'n1',
  kind: { kind: 'Output' as const, label: 'Out' },
  position: { x: 1, y: 2 },
};

const flowTab = (id: string, flowName: string, patch: Partial<FlowTab> = {}): FlowTab => ({
  id,
  title: `Flow: ${flowName}`,
  isDirty: false,
  tabType: 'flow',
  collectionName: 'c',
  flowName,
  nodes: [],
  edges: [],
  callbackHost: null,
  nodeStatus: {},
  runState: 'idle',
  ...patch,
});

const auth = { authType: 'bearer' } as AuthState;
const key = (flow: string) => flowAuthKey('c', flow, 'a1', null, null);
const liveTab = (id: string) =>
  collectAllTabs(usePaneStore.getState().root)
    .filter(isFlowTab)
    .find((t) => t.id === id);

describe('renameFlowTabs', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    useFlowAuthStore.setState({ auths: {} });
  });

  it('retargets a dirty tab and keeps its id and unsaved edits', () => {
    usePaneStore
      .getState()
      .openTab(
        flowTab('t1', 'Login', { isDirty: true, nodes: [node], nodeStatus: { n1: 'success' } }),
      );

    usePaneStore.getState().renameFlowTabs('c', 'Login', 'Sign In');

    const tab = liveTab('t1');
    expect(tab?.flowName).toBe('Sign In');
    expect(tab?.title).toBe('Flow: Sign In');
    expect(tab?.isDirty).toBe(true);
    expect(tab?.nodes).toEqual([node]);
    expect(tab?.nodeStatus).toEqual({ n1: 'success' });
  });

  it('leaves other flows and other collections alone', () => {
    usePaneStore.getState().openTab(flowTab('t1', 'Login'));
    usePaneStore.getState().openTab(flowTab('t2', 'login'));
    usePaneStore.getState().openTab(flowTab('t3', 'Login', { collectionName: 'other' }));

    usePaneStore.getState().renameFlowTabs('c', 'Login', 'Sign In');

    expect(liveTab('t1')?.flowName).toBe('Sign In');
    expect(liveTab('t2')?.flowName).toBe('login');
    expect(liveTab('t3')?.flowName).toBe('Login');
  });

  it('retargets a tab parked in a collection snapshot', () => {
    usePaneStore.setState({
      collectionTabState: {
        other: { tabs: [flowTab('p1', 'Login', { isDirty: true })], activeTabId: 'p1' },
      },
    });

    usePaneStore.getState().renameFlowTabs('c', 'Login', 'Sign In');

    const parked = usePaneStore.getState().collectionTabState.other?.tabs[0];
    expect(parked && isFlowTab(parked) ? parked.flowName : null).toBe('Sign In');
    expect(parked?.isDirty).toBe(true);
  });

  it('clears the old name tokens and the new name tokens, and keeps other flows', () => {
    useFlowAuthStore.setState({
      auths: {
        [key('Login')]: { auth },
        [key('Sign In')]: { auth },
        [key('Other')]: { auth },
      },
    });
    usePaneStore.getState().openTab(flowTab('t1', 'Login'));

    usePaneStore.getState().renameFlowTabs('c', 'Login', 'Sign In');

    expect(Object.keys(useFlowAuthStore.getState().auths)).toEqual([key('Other')]);
  });

  it('clears the tokens even when no tab of the flow is open', () => {
    useFlowAuthStore.setState({ auths: { [key('Login')]: { auth } } });
    usePaneStore.getState().renameFlowTabs('c', 'Login', 'Sign In');
    expect(useFlowAuthStore.getState().auths).toEqual({});
  });
});

describe('dropParkedFlowTabs', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
  });

  it('removes only the matching flow tabs from snapshots and fixes the active id', () => {
    usePaneStore.setState({
      collectionTabState: {
        other: {
          tabs: [flowTab('p1', 'Login'), flowTab('p2', 'Sync')],
          activeTabId: 'p1',
        },
      },
    });

    usePaneStore.getState().dropParkedFlowTabs('c', 'Login');

    const entry = usePaneStore.getState().collectionTabState.other;
    expect(entry?.tabs.map((t) => t.id)).toEqual(['p2']);
    expect(entry?.activeTabId).toBe('p2');
  });
});

describe('dropParkedFlowTabs without a match', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
  });

  it('keeps the snapshot state object when nothing matched', () => {
    usePaneStore.setState({
      collectionTabState: { other: { tabs: [flowTab('p2', 'Sync')], activeTabId: 'p2' } },
    });
    const before = usePaneStore.getState().collectionTabState;

    usePaneStore.getState().dropParkedFlowTabs('c', 'Login');

    expect(usePaneStore.getState().collectionTabState).toBe(before);
  });
});

describe('openFlowTab', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.mocked(getFlow).mockReset().mockResolvedValue({ name: 'Login', nodes: [], edges: [] });
  });

  const liveFlowTabs = () => collectAllTabs(usePaneStore.getState().root).filter(isFlowTab);

  it('opens one tab when the same flow is opened twice at once', async () => {
    await Promise.all([
      usePaneStore.getState().openFlowTab('c', 'Login'),
      usePaneStore.getState().openFlowTab('c', 'Login'),
    ]);
    expect(liveFlowTabs()).toHaveLength(1);
  });

  it('brings a dirty parked tab back instead of opening a copy from disk', async () => {
    usePaneStore.setState({
      collectionTabState: {
        other: { tabs: [flowTab('p1', 'Login', { isDirty: true })], activeTabId: 'p1' },
      },
    });

    await usePaneStore.getState().openFlowTab('c', 'Login');

    const tabs = liveFlowTabs();
    expect(tabs).toHaveLength(1);
    expect(tabs[0]?.id).toBe('p1');
    expect(tabs[0]?.isDirty).toBe(true);
    expect(usePaneStore.getState().collectionTabState.other?.tabs).toEqual([]);
    expect(getFlow).not.toHaveBeenCalled();
  });
});
