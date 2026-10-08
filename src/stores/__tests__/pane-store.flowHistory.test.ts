import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { findTabInTree } from '@/lib/pane-utils';
import type { FlowNode } from '@/lib/tauri-api';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { usePaneStore } from '../pane-store';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn(), getFlow: vi.fn(), endAgentSession: vi.fn() };
});

const TAB_ID = 'flow-history-1';

const node = (id: string, x = 0): FlowNode => ({
  id,
  kind: { kind: 'Output', label: id },
  position: { x, y: 0 },
});

function openFlow(over: Partial<FlowTab> = {}): FlowTab {
  const tab: FlowTab = {
    id: TAB_ID,
    title: 'Flow: h',
    isDirty: false,
    tabType: 'flow',
    collectionName: 'demo',
    flowName: 'h',
    nodes: [node('a'), node('b', 100)],
    edges: [],
    nodeStatus: {},
    runState: 'idle',
    ...over,
  };
  usePaneStore.getState().openTab(tab);
  return tab;
}

function current(): FlowTab {
  const found = findTabInTree(usePaneStore.getState().root, TAB_ID);
  if (!found || !isFlowTab(found.tab)) throw new Error('Expected the flow tab');
  return found.tab;
}

const store = () => usePaneStore.getState();
const ids = () => current().nodes.map((n) => n.id);
const steps = () => current().history?.past.length ?? 0;

describe('flow tab history', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-10-08T10:00:00Z'));
    store().reset();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('records one step per write and clears redo on a new edit', () => {
    openFlow();
    store().updateFlowNodes(TAB_ID, [node('a')]);
    store().updateFlowNodes(TAB_ID, [node('a'), node('c')]);
    expect(steps()).toBe(2);
    store().undoFlow(TAB_ID);
    expect(current().history?.future).toHaveLength(1);
    store().updateFlowNodes(TAB_ID, [node('z')]);
    expect(current().history?.future).toHaveLength(0);
  });

  it('undoes and redoes nodes, edges and the callback host together', () => {
    const tab = openFlow();
    const edge = {
      id: 'e1',
      sourceNodeId: 'a',
      targetNodeId: 'b',
      targetField: 'value',
      expression: 'response.body',
    };
    store().updateFlowGraph(TAB_ID, [node('a')], [edge]);
    store().setFlowCallbackHost(TAB_ID, '10.0.0.5');
    store().undoFlow(TAB_ID);
    expect(current().callbackHost ?? null).toBeNull();
    expect(current().edges).toEqual([edge]);
    store().undoFlow(TAB_ID);
    expect(current().nodes).toBe(tab.nodes);
    expect(current().edges).toBe(tab.edges);
    store().redoFlow(TAB_ID);
    store().redoFlow(TAB_ID);
    expect(current().callbackHost).toBe('10.0.0.5');
    expect(ids()).toEqual(['a']);
  });

  it('makes updateFlowGraph exactly one step', () => {
    openFlow();
    store().updateFlowGraph(TAB_ID, [node('a')], []);
    expect(steps()).toBe(1);
    store().undoFlow(TAB_ID);
    expect(ids()).toEqual(['a', 'b']);
  });

  it('does nothing for undo and redo with no history', () => {
    const tab = openFlow();
    store().undoFlow(TAB_ID);
    store().redoFlow(TAB_ID);
    expect(current().nodes).toBe(tab.nodes);
    expect(current().isDirty).toBe(false);
  });

  it('records nothing for a write that changes nothing', () => {
    const tab = openFlow();
    store().updateFlowNodes(TAB_ID, tab.nodes);
    expect(steps()).toBe(0);
    expect(current().isDirty).toBe(true);
  });

  it('keeps at most 100 steps', () => {
    openFlow();
    for (let i = 0; i < 105; i += 1) store().updateFlowNodes(TAB_ID, [node(`n${i}`)]);
    expect(steps()).toBe(100);
  });

  it('coalesces writes with one key inside the window and splits after it', () => {
    openFlow();
    const opts = { coalesceKey: 'kind:a' };
    store().updateFlowNodes(TAB_ID, [node('a', 1)], opts);
    vi.advanceTimersByTime(300);
    store().updateFlowNodes(TAB_ID, [node('a', 2)], opts);
    vi.advanceTimersByTime(300);
    store().updateFlowNodes(TAB_ID, [node('a', 3)], opts);
    expect(steps()).toBe(1);
    vi.advanceTimersByTime(601);
    store().updateFlowNodes(TAB_ID, [node('a', 4)], opts);
    expect(steps()).toBe(2);
    store().undoFlow(TAB_ID);
    expect(current().nodes[0].position.x).toBe(3);
    store().undoFlow(TAB_ID);
    expect(ids()).toEqual(['a', 'b']);
    expect(current().nodes[0].position.x).toBe(0);
  });

  it('does not coalesce different keys', () => {
    openFlow();
    store().updateFlowNodes(TAB_ID, [node('a', 1)], { coalesceKey: 'kind:a' });
    store().updateFlowNodes(TAB_ID, [node('a', 2)], { coalesceKey: 'kind:b' });
    expect(steps()).toBe(2);
  });

  it('folds the node write and the edge write of one Delete press into one step', () => {
    const edge = {
      id: 'e1',
      sourceNodeId: 'a',
      targetNodeId: 'b',
      targetField: 'value',
      expression: 'response.body',
    };
    openFlow({ edges: [edge] });
    const remove = { coalesceKey: 'canvas-remove', coalesceMs: 50 };
    store().updateFlowNodes(TAB_ID, [], remove);
    store().updateFlowEdges(TAB_ID, [], remove);
    expect(steps()).toBe(1);
    store().undoFlow(TAB_ID);
    expect(ids()).toEqual(['a', 'b']);
    expect(current().edges).toEqual([edge]);
  });

  it('makes a whole drag one step, created by its first move', () => {
    openFlow();
    store().beginFlowGesture(TAB_ID);
    expect(steps()).toBe(0);
    for (let x = 1; x <= 20; x += 1) {
      store().updateFlowNodes(TAB_ID, [node('a', x), node('b', 100)], { gesture: true });
    }
    store().endFlowGesture(TAB_ID);
    expect(steps()).toBe(1);
    store().undoFlow(TAB_ID);
    expect(current().nodes[0].position.x).toBe(0);
    store().redoFlow(TAB_ID);
    expect(current().nodes[0].position.x).toBe(20);
  });

  it('makes no step for a gesture without a move, and one step per later gesture', () => {
    openFlow();
    store().beginFlowGesture(TAB_ID);
    store().endFlowGesture(TAB_ID);
    expect(steps()).toBe(0);
    for (const x of [10, 30]) {
      store().beginFlowGesture(TAB_ID);
      store().updateFlowNodes(TAB_ID, [node('a', x), node('b', 100)], { gesture: true });
      store().updateFlowNodes(TAB_ID, [node('a', x + 1), node('b', 100)], { gesture: true });
      store().endFlowGesture(TAB_ID);
    }
    expect(steps()).toBe(2);
  });

  it('leaves history alone for status, progress and run state changes', () => {
    openFlow();
    store().updateFlowNodes(TAB_ID, [node('a'), node('c')]);
    const before = current().history;
    store().patchFlowNodeStatus(TAB_ID, 'a', 'running');
    store().patchFlowNodeProgress(TAB_ID, 'a', 'attempt 1/3');
    store().setFlowRunState(TAB_ID, 'running', 'run-1');
    store().patchFlowNodeStatus(TAB_ID, 'a', 'success', { statusCode: 200 });
    store().setFlowRunState(TAB_ID, 'done', 'run-1');
    expect(current().history).toBe(before);
    store().undoFlow(TAB_ID);
    expect(ids()).toEqual(['a', 'b']);
  });

  describe('dirty flag', () => {
    it('is clear again after undoing back to the loaded state', () => {
      openFlow();
      store().updateFlowNodes(TAB_ID, [node('a')]);
      expect(current().isDirty).toBe(true);
      store().undoFlow(TAB_ID);
      expect(current().isDirty).toBe(false);
      store().redoFlow(TAB_ID);
      expect(current().isDirty).toBe(true);
    });

    it('is clear when undoing back to the last save, and set when undoing past it', () => {
      openFlow();
      store().updateFlowNodes(TAB_ID, [node('a')]);
      store().markClean(TAB_ID);
      expect(current().isDirty).toBe(false);
      store().updateFlowNodes(TAB_ID, [node('a'), node('c')]);
      expect(current().isDirty).toBe(true);
      store().undoFlow(TAB_ID);
      expect(current().isDirty).toBe(false);
      store().undoFlow(TAB_ID);
      expect(current().isDirty).toBe(true);
      expect(ids()).toEqual(['a', 'b']);
    });

    it('is set when the first undo goes back from a just-saved state', () => {
      openFlow();
      store().updateFlowNodes(TAB_ID, [node('a')]);
      store().markClean(TAB_ID);
      store().undoFlow(TAB_ID);
      expect(current().isDirty).toBe(true);
    });

    it('stays dirty for a tab whose loaded state is unknown', () => {
      openFlow({ isDirty: true });
      store().updateFlowNodes(TAB_ID, [node('a')]);
      store().undoFlow(TAB_ID);
      expect(current().isDirty).toBe(true);
    });
  });
});
