import { act, render, screen } from '@testing-library/react';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DEFAULT_AUTH_NODE_AUTH } from '@/lib/flow-auth';
import { clearFlowClipboard, getFlowClipboard, PASTE_OFFSET } from '@/lib/flow-clipboard';
import { caseHandle } from '@/lib/flow-handles';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

// The Request editor pulls in Monaco, which jsdom cannot load.
vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    lintFlow: vi.fn().mockResolvedValue([]),
    listCollections: vi.fn().mockResolvedValue([]),
    listFlows: vi.fn().mockResolvedValue([]),
  };
});

vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() } }));

vi.mock('@/components/editor', () => ({ SingleLineEditor: () => null }));

// jsdom cannot drive React Flow's gestures, so the canvas is a stand-in that
// exposes the props FlowPane gives it.
interface CanvasProps {
  selectedNodeIds?: ReadonlySet<string>;
  onSelectedNodeIdsChange?: (ids: ReadonlySet<string>) => void;
  onCopy?: () => void;
  onPaste?: () => void;
  onDuplicate?: () => void;
  onDuplicateNode?: (nodeId: string) => void;
}
let canvas: CanvasProps = {};
vi.mock('../FlowCanvas', () => ({
  FlowCanvas: (props: CanvasProps) => {
    canvas = props;
    return null;
  },
}));

const TAB_ID = 'flow-clip-1';

const node = (id: string, kind: FlowNode['kind'], x = 0, y = 0): FlowNode => ({
  id,
  kind,
  position: { x, y },
});
const output = (id: string, x = 0, y = 0) => node(id, { kind: 'Output', label: id }, x, y);
const wire = (id: string, from: string, to: string, over: Partial<FlowEdge> = {}): FlowEdge => ({
  id,
  sourceNodeId: from,
  targetNodeId: to,
  targetField: 'value',
  expression: 'response.body',
  ...over,
});

function seed(nodes: FlowNode[], edges: FlowEdge[] = [], over: Partial<FlowTab> = {}) {
  usePaneStore.getState().openTab({
    id: TAB_ID,
    tabType: 'flow',
    title: 'Flow: clip',
    isDirty: false,
    collectionName: 'demo',
    flowName: 'clip',
    nodes,
    edges,
    nodeStatus: {},
    runState: 'idle',
    ...over,
  });
}

function current(): FlowTab {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected root to be a leaf');
  const tab = root.tabs.find((t) => t.id === TAB_ID);
  if (!tab || !isFlowTab(tab)) throw new Error('Expected the flow tab');
  return tab;
}

function Harness() {
  const tab = usePaneStore((s) => {
    const root = s.root;
    if (root.type !== 'leaf') return null;
    const t = root.tabs.find((x) => x.id === TAB_ID);
    return t && isFlowTab(t) ? t : null;
  });
  if (!tab) return null;
  return <FlowPane tab={tab} groupId={usePaneStore.getState().activeGroupId} />;
}

const select = (...ids: string[]) => act(() => canvas.onSelectedNodeIdsChange?.(new Set(ids)));

describe('FlowPane copy, paste and duplicate', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    clearFlowClipboard();
    canvas = {};
  });

  it('pastes copied nodes and wires as new nodes and selects them', () => {
    seed(
      [output('a', 0, 0), output('b', 100, 0)],
      [wire('e1', 'a', 'b', { targetField: 'trigger' })],
    );
    render(<Harness />);
    select('a', 'b');
    act(() => canvas.onCopy?.());
    expect(getFlowClipboard()?.nodes).toHaveLength(2);
    act(() => canvas.onPaste?.());
    const tab = current();
    expect(tab.nodes).toHaveLength(4);
    expect(tab.edges).toHaveLength(2);
    const added = tab.nodes.slice(2);
    expect(added.map((n) => n.position)).toEqual([
      { x: PASTE_OFFSET, y: PASTE_OFFSET },
      { x: 100 + PASTE_OFFSET, y: PASTE_OFFSET },
    ]);
    expect(tab.edges[1].sourceNodeId).toBe(added[0].id);
    expect(tab.edges[1].targetNodeId).toBe(added[1].id);
    expect(canvas.selectedNodeIds).toEqual(new Set(added.map((n) => n.id)));
    expect(tab.isDirty).toBe(true);
  });

  it('makes one paste one undo step', () => {
    seed([output('a'), output('b', 100)], [wire('e1', 'a', 'b', { targetField: 'trigger' })]);
    render(<Harness />);
    select('a', 'b');
    act(() => canvas.onCopy?.());
    act(() => canvas.onPaste?.());
    expect(current().history?.past).toHaveLength(1);
    act(() => usePaneStore.getState().undoFlow(TAB_ID));
    expect(current().nodes.map((n) => n.id)).toEqual(['a', 'b']);
    expect(current().edges.map((e) => e.id)).toEqual(['e1']);
  });

  it('moves each repeated paste further away', () => {
    seed([output('a', 0, 0)]);
    render(<Harness />);
    select('a');
    act(() => canvas.onCopy?.());
    act(() => canvas.onPaste?.());
    act(() => canvas.onPaste?.());
    const [, first, second] = current().nodes;
    expect(first.position).toEqual({ x: PASTE_OFFSET, y: PASTE_OFFSET });
    expect(second.position).toEqual({ x: 2 * PASTE_OFFSET, y: 2 * PASTE_OFFSET });
  });

  it('does nothing when nothing is selected or the clipboard is empty', () => {
    seed([output('a')]);
    render(<Harness />);
    act(() => canvas.onCopy?.());
    expect(getFlowClipboard()).toBeNull();
    act(() => canvas.onPaste?.());
    act(() => canvas.onDuplicate?.());
    expect(current().nodes).toHaveLength(1);
    expect(current().isDirty).toBe(false);
  });

  it('pastes into another flow tab through the shared clipboard', () => {
    seed([output('a', 5, 5)]);
    const first = render(<Harness />);
    select('a');
    act(() => canvas.onCopy?.());
    first.unmount();
    usePaneStore.getState().reset();
    seed([output('z')]);
    render(<Harness />);
    act(() => canvas.onPaste?.());
    expect(current().nodes).toHaveLength(2);
    expect(current().nodes[0].id).toBe('z');
    expect(current().nodes[1].position).toEqual({ x: 5 + PASTE_OFFSET, y: 5 + PASTE_OFFSET });
  });

  it('refuses saved requests from another collection', () => {
    const saved = node('s', {
      kind: 'Request',
      label: 'S',
      source: { type: 'Saved', requestPath: 'a.yml' },
    });
    seed([saved], [], { collectionName: 'one' });
    const first = render(<Harness />);
    select('s');
    act(() => canvas.onCopy?.());
    first.unmount();
    usePaneStore.getState().reset();
    seed([output('z')], [], { collectionName: 'two' });
    render(<Harness />);
    act(() => canvas.onPaste?.());
    expect(current().nodes).toHaveLength(1);
    expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('one'));
  });

  it('duplicates the selection without touching the clipboard', () => {
    seed([output('a', 0, 0)]);
    render(<Harness />);
    select('a');
    act(() => canvas.onDuplicate?.());
    expect(current().nodes).toHaveLength(2);
    expect(getFlowClipboard()).toBeNull();
    expect(canvas.selectedNodeIds?.size).toBe(1);
    expect(canvas.selectedNodeIds?.has('a')).toBe(false);
  });

  it('duplicates one node from its menu even when it is not selected', () => {
    seed([output('a'), output('b', 100)]);
    render(<Harness />);
    select('b');
    act(() => canvas.onDuplicateNode?.('a'));
    expect(current().nodes).toHaveLength(3);
    expect(current().nodes[2].kind).toEqual({ kind: 'Output', label: 'a' });
  });

  it('pastes an Auth node that does not apply to inherited auth and tells the user', () => {
    const auth = node('auth', {
      kind: 'Auth',
      label: 'Auth',
      auth: DEFAULT_AUTH_NODE_AUTH,
      applyToInherit: true,
    });
    seed([auth]);
    render(<Harness />);
    select('auth');
    act(() => canvas.onDuplicate?.());
    const kinds = current().nodes.map((n) => n.kind);
    expect(kinds.filter((k) => k.kind === 'Auth' && k.applyToInherit)).toHaveLength(1);
    expect(toast.info).toHaveBeenCalledWith(expect.stringMatching(/inherited auth/i));
  });

  it('keeps a pasted Switch wired to its own case exits', () => {
    const sw = node('sw', {
      kind: 'Switch',
      label: 'Route',
      value: 'x',
      cases: [{ id: 'c1', label: 'One', matches: '1' }],
    });
    seed(
      [sw, output('o1', 300, 0)],
      [wire('e1', 'sw', 'o1', { sourceHandle: caseHandle('c1'), targetField: 'trigger' })],
    );
    render(<Harness />);
    select('sw', 'o1');
    act(() => canvas.onDuplicate?.());
    const tab = current();
    const pasted = tab.nodes[2].kind;
    if (pasted.kind !== 'Switch') throw new Error('Expected a Switch node');
    expect(pasted.cases[0].id).not.toBe('c1');
    expect(tab.edges[1].sourceHandle).toBe(caseHandle(pasted.cases[0].id));
    expect(tab.edges[0].sourceHandle).toBe(caseHandle('c1'));
  });

  it('renames a pasted Wait for callback and warns about the old variable', () => {
    const wait = node('w', {
      kind: 'WaitForCallback',
      label: 'Wait',
      name: 'pay',
      timeoutMs: 60000,
    });
    seed([wait]);
    render(<Harness />);
    select('w');
    act(() => canvas.onDuplicate?.());
    const names = current().nodes.map((n) =>
      n.kind.kind === 'WaitForCallback' ? n.kind.name : '',
    );
    expect(names).toEqual(['pay', 'pay_2']);
    expect(toast.info).toHaveBeenCalledWith(expect.stringContaining('{{callback.pay}}'));
  });

  it('does not open the properties panel for the pasted nodes', () => {
    seed([output('a')]);
    render(<Harness />);
    select('a');
    act(() => canvas.onDuplicate?.());
    expect(screen.queryByTestId('node-properties-panel')).not.toBeInTheDocument();
  });
});
