import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook } from '@testing-library/react';
import type { ReactNode } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { sendRequest } from '@/lib/execute-request';
import { usePaneStore } from '@/stores/pane-store';
import type { FlowTab } from '@/types/pane-types';
import { useKeyboardShortcuts } from '../useKeyboardShortcuts';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/execute-request', () => ({ sendRequest: vi.fn() }));

const wrapper = ({ children }: { children: ReactNode }) => (
  <QueryClientProvider client={new QueryClient()}>{children}</QueryClientProvider>
);

const flowTab: FlowTab = {
  id: 'flow-run-1',
  title: 'Flow: my-flow',
  isDirty: false,
  tabType: 'flow',
  collectionName: 'demo',
  flowName: 'my-flow',
  nodes: [],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
};

function pressCtrlEnter() {
  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', ctrlKey: true }));
}

describe('Ctrl+Enter on a flow tab', () => {
  let runRequests: string[];
  const listener = (e: Event) => runRequests.push((e as CustomEvent).detail.tabId);

  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
    runRequests = [];
    window.addEventListener('rocket:flow-run', listener);
  });
  afterEach(() => window.removeEventListener('rocket:flow-run', listener));

  it('asks the flow toolbar to run', () => {
    usePaneStore.getState().openTab(flowTab);
    renderHook(() => useKeyboardShortcuts(), { wrapper });
    pressCtrlEnter();
    expect(runRequests).toEqual(['flow-run-1']);
    expect(sendRequest).not.toHaveBeenCalled();
  });

  it('does not ask for a flow run from a request tab', () => {
    usePaneStore.getState().openEphemeralTab();
    renderHook(() => useKeyboardShortcuts(), { wrapper });
    pressCtrlEnter();
    expect(runRequests).toEqual([]);
    expect(sendRequest).toHaveBeenCalledTimes(1);
  });
});
