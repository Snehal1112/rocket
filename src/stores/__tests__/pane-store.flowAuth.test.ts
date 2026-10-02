import { beforeEach, describe, expect, it, vi } from 'vitest';
import { flowAuthKey } from '@/lib/flow-auth';
import type { AuthState, FlowTab } from '@/types/pane-types';
import { useFlowAuthStore } from '../flow-auth-store';
import { usePaneStore } from '../pane-store';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, endAgentSession: vi.fn() };
});

const flowTab = (id: string, flowName: string): FlowTab => ({
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
});

const key = (flow: string) => flowAuthKey('c', flow, 'a1', null, null);

describe('closing a flow tab clears its Auth tokens', () => {
  let groupId: string;
  beforeEach(() => {
    usePaneStore.getState().reset();
    const auth = { authType: 'bearer' } as AuthState;
    useFlowAuthStore.setState({ auths: { [key('f')]: { auth }, [key('g')]: { auth } } });
    usePaneStore.getState().openTab(flowTab('t1', 'f'));
    usePaneStore.getState().openTab(flowTab('t2', 'f'));
    usePaneStore.getState().openTab(flowTab('t3', 'g'));
    groupId = usePaneStore.getState().activeGroupId;
  });

  it('keeps the tokens while another tab shows the same flow', () => {
    usePaneStore.getState().closeTab('t1', groupId);
    expect(Object.keys(useFlowAuthStore.getState().auths)).toHaveLength(2);
  });

  it('clears that flow, and only that flow, when its last tab closes', () => {
    usePaneStore.getState().closeTab('t1', groupId);
    usePaneStore.getState().closeTab('t2', groupId);
    expect(Object.keys(useFlowAuthStore.getState().auths)).toEqual([key('g')]);
  });
});

describe('bulk tab drops', () => {
  const auth = { authType: 'bearer' } as AuthState;
  beforeEach(() => {
    usePaneStore.getState().reset();
    useFlowAuthStore.setState({ auths: { [key('f')]: { auth }, [key('g')]: { auth } } });
    usePaneStore.getState().openTab(flowTab('t1', 'f'));
    usePaneStore.getState().openTab(flowTab('t3', 'g'));
  });

  it('closeAll clears the tokens of flow tabs', () => {
    usePaneStore.getState().closeAll();
    expect(Object.keys(useFlowAuthStore.getState().auths)).toEqual([]);
  });

  it('reset clears the tokens of flow tabs', () => {
    usePaneStore.getState().reset();
    expect(Object.keys(useFlowAuthStore.getState().auths)).toEqual([]);
  });

  it('switchCollection snapshots the tabs and does not clear', () => {
    usePaneStore.setState({ activeCollection: 'c' });
    usePaneStore.getState().switchCollection('d');
    expect(Object.keys(useFlowAuthStore.getState().auths)).toHaveLength(2);
  });
});
