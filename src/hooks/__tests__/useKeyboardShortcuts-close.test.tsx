import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook } from '@testing-library/react';
import type { ReactNode } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { usePaneStore } from '@/stores/pane-store';
import type { FlowTab } from '@/types/pane-types';
import { useKeyboardShortcuts } from '../useKeyboardShortcuts';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, readScriptFile: vi.fn().mockResolvedValue('one') };
});

const wrapper = ({ children }: { children: ReactNode }) => (
  <QueryClientProvider client={new QueryClient()}>{children}</QueryClientProvider>
);

const flowTab = (isDirty: boolean): FlowTab => ({
  id: 'flow-close-1',
  title: 'Flow: my-flow',
  isDirty,
  tabType: 'flow',
  collectionName: 'demo',
  flowName: 'my-flow',
  nodes: [],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
});

function pressCtrlW() {
  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'w', ctrlKey: true }));
}

function activeTabId(): string {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected leaf root');
  return root.activeTabId;
}

describe('Ctrl+W close guard', () => {
  const realCloseTab = usePaneStore.getState().closeTab;
  let closeTab: ReturnType<typeof vi.fn>;
  let requested: string[];
  const listener = (e: Event) => requested.push((e as CustomEvent).detail.tabId);

  beforeEach(() => {
    usePaneStore.getState().reset();
    closeTab = vi.fn();
    usePaneStore.setState({ closeTab } as never);
    requested = [];
    window.addEventListener('rocket:request-close-tab', listener);
  });
  afterEach(() => {
    window.removeEventListener('rocket:request-close-tab', listener);
    usePaneStore.setState({ closeTab: realCloseTab });
  });

  it('requests a guarded close for a dirty script tab', async () => {
    await usePaneStore.getState().openScriptTab('col', 'a.js');
    const id = activeTabId();
    usePaneStore.getState().updateScriptContent(id, 'two');
    renderHook(() => useKeyboardShortcuts(), { wrapper });
    pressCtrlW();
    expect(requested).toEqual([id]);
    expect(closeTab).not.toHaveBeenCalled();
  });

  it('closes a clean script tab directly', async () => {
    await usePaneStore.getState().openScriptTab('col', 'a.js');
    const id = activeTabId();
    renderHook(() => useKeyboardShortcuts(), { wrapper });
    pressCtrlW();
    expect(requested).toEqual([]);
    expect(closeTab).toHaveBeenCalledWith(id, usePaneStore.getState().activeGroupId);
  });

  it('requests a guarded close for a dirty flow tab', () => {
    usePaneStore.getState().openTab(flowTab(true));
    renderHook(() => useKeyboardShortcuts(), { wrapper });
    pressCtrlW();
    expect(requested).toEqual(['flow-close-1']);
    expect(closeTab).not.toHaveBeenCalled();
  });

  it('closes a clean flow tab directly', () => {
    usePaneStore.getState().openTab(flowTab(false));
    renderHook(() => useKeyboardShortcuts(), { wrapper });
    pressCtrlW();
    expect(requested).toEqual([]);
    expect(closeTab).toHaveBeenCalledWith('flow-close-1', usePaneStore.getState().activeGroupId);
  });

  it.each([false, true])('closes a request tab directly (dirty=%s)', (dirty) => {
    usePaneStore.getState().openEphemeralTab();
    const id = activeTabId();
    if (dirty) usePaneStore.getState().markDirty(id);
    renderHook(() => useKeyboardShortcuts(), { wrapper });
    pressCtrlW();
    expect(requested).toEqual([]);
    expect(closeTab).toHaveBeenCalledWith(id, usePaneStore.getState().activeGroupId);
  });
});
