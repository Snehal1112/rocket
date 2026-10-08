import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

// A stub canvas lets the test fire a connection, which jsdom cannot drag.
vi.mock('../FlowCanvas', () => ({
  FlowCanvas: ({ onConnect }: { onConnect: (c: unknown) => void }) => (
    <button
      type='button'
      onClick={() =>
        onConnect({ source: 'in1', target: 'req1', sourceHandle: null, targetHandle: 'headers' })
      }
    >
      connect-headers
    </button>
  ),
}));

vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: ({ value, onChange }: { value: string; onChange?: (v: string) => void }) => (
    <textarea aria-label='Wire script' value={value} onChange={(e) => onChange?.(e.target.value)} />
  ),
}));

vi.mock('@/components/editor', () => ({
  SingleLineEditor: ({ value, onChange }: { value: string; onChange: (v: string) => void }) => (
    <input value={value} onChange={(e) => onChange(e.target.value)} />
  ),
}));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    lintFlow: vi.fn().mockResolvedValue([]),
    listCollections: vi.fn().mockResolvedValue([]),
    listFlows: vi.fn().mockResolvedValue([]),
    saveFlow: vi.fn().mockResolvedValue(undefined),
  };
});

vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn() } }));

Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.releasePointerCapture ??= () => undefined;
Element.prototype.scrollIntoView ??= () => undefined;

const baseTab: FlowTab = {
  id: 'flow-wireundo-1',
  tabType: 'flow',
  title: 'Flow: wireundo',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'wireundo',
  nodes: [
    { id: 'in1', position: { x: 0, y: 0 }, kind: { kind: 'Input', label: 'User', value: 'a' } },
    {
      id: 'req1',
      position: { x: 300, y: 0 },
      kind: {
        kind: 'Request',
        label: 'Login',
        source: { type: 'Inline', request: { method: 'GET', url: '', headers: [] } },
      },
    },
  ],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
};

function getTab(): FlowTab {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected root to be a leaf');
  const tab = root.tabs.find((t) => t.id === baseTab.id);
  if (!tab || !isFlowTab(tab)) throw new Error('Expected the flow tab');
  return tab;
}

function Harness() {
  const tab = usePaneStore((s) => {
    if (s.root.type !== 'leaf') return null;
    const t = s.root.tabs.find((x) => x.id === baseTab.id);
    return t && isFlowTab(t) ? t : null;
  });
  if (!tab) return null;
  return <FlowPane tab={tab} groupId={usePaneStore.getState().activeGroupId} />;
}

describe('FlowPane wire undo and save', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
  });

  it('undoes a headers connect and its commit in one step', async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    render(<Harness />);
    await user.click(screen.getByText('connect-headers'));
    expect(getTab().edges.map((e) => e.targetField)).toEqual(['headers']);
    await user.type(await screen.findByLabelText('Header name'), 'X-Token');
    await user.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(getTab().edges[0].targetField).toBe('headers[X-Token].value'));

    await user.click(screen.getByRole('button', { name: 'Undo' }));
    expect(getTab().edges).toEqual([]);
    expect(getTab().isDirty).toBe(false);
  });

  it('undoes a headers connect and its dismissal in one step', async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    render(<Harness />);
    await user.click(screen.getByText('connect-headers'));
    await user.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(getTab().edges).toEqual([]));

    await user.click(screen.getByRole('button', { name: 'Undo' }));
    expect(getTab().edges).toEqual([]);
    expect(screen.getByRole('button', { name: 'Undo' })).toBeDisabled();
  });

  it('keeps the tab dirty when an edit lands while a save is in flight', async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    let resolveSave: () => void = () => undefined;
    vi.mocked(tauriApi.saveFlow).mockImplementationOnce(
      () => new Promise<void>((r) => { resolveSave = r; }),
    );
    render(<Harness />);
    act(() => {
      usePaneStore.getState().updateFlowNodes(baseTab.id, baseTab.nodes.slice(0, 1));
    });
    const written = getTab().nodes;
    await user.click(screen.getAllByRole('button', { name: 'Save' })[0]);
    act(() => {
      usePaneStore.getState().updateFlowNodes(baseTab.id, baseTab.nodes);
    });
    await act(async () => {
      resolveSave();
    });
    expect(getTab().isDirty).toBe(true);

    fireEvent.click(screen.getByRole('button', { name: 'Undo' }));
    expect(getTab().nodes).toBe(written);
    expect(getTab().isDirty).toBe(false);
  });
});
