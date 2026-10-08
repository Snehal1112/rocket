import type { FlowEdge, FlowNode } from '@/lib/tauri-api';

export const HISTORY_LIMIT = 100;
// Writes with one key inside this window after the previous one share a step.
export const DEFAULT_COALESCE_MS = 600;

/** The part of a flow tab that undo restores. Arrays are never mutated, so references identify a state. */
export interface GraphSnap {
  nodes: FlowNode[];
  edges: FlowEdge[];
  callbackHost?: string | null;
}

export interface FlowHistory {
  past: GraphSnap[];
  future: GraphSnap[];
  /** The graph as last saved or loaded, so undo back to it clears the dirty dot. */
  saved?: GraphSnap;
  /** Key and time of the last coalescible write. */
  lastKey?: string;
  lastAt?: number;
  /** Set between `beginFlowGesture` and `endFlowGesture`. `recorded` once its step is pushed. */
  gesture?: 'armed' | 'recorded';
}

export interface FlowWriteOptions {
  /** Writes with the same key in a row make one undo step. */
  coalesceKey?: string;
  /** Window for `coalesceKey`. Defaults to `DEFAULT_COALESCE_MS`. */
  coalesceMs?: number;
  /** The write belongs to a drag. Only the first one since `beginFlowGesture` makes a step. */
  gesture?: boolean;
}

export const emptyHistory = (): FlowHistory => ({ past: [], future: [] });

export function snapOf(graph: {
  nodes: FlowNode[];
  edges: FlowEdge[];
  callbackHost?: string | null;
}): GraphSnap {
  return { nodes: graph.nodes, edges: graph.edges, callbackHost: graph.callbackHost };
}

function pushPast(history: FlowHistory, before: GraphSnap): FlowHistory {
  return { ...history, past: [...history.past, before].slice(-HISTORY_LIMIT), future: [] };
}

/**
 * Returns the history after a graph write. `before` is the graph as it was
 * before the write. A write that is part of the current step records nothing.
 */
export function recordEdit(
  history: FlowHistory | undefined,
  before: GraphSnap,
  options: FlowWriteOptions | undefined,
  now: number,
): FlowHistory {
  const h = history ?? emptyHistory();
  if (options?.gesture) {
    if (h.gesture === 'recorded') return h;
    return { ...pushPast(h, before), lastKey: undefined, lastAt: undefined, gesture: 'recorded' };
  }
  const key = options?.coalesceKey;
  if (
    key !== undefined &&
    h.lastKey === key &&
    h.lastAt !== undefined &&
    now - h.lastAt <= (options?.coalesceMs ?? DEFAULT_COALESCE_MS)
  ) {
    return { ...h, lastAt: now };
  }
  return {
    ...pushPast(h, before),
    lastKey: key,
    lastAt: key === undefined ? undefined : now,
  };
}

/** Moves one step back. Null when there is nothing to undo. */
export function undoStep(
  history: FlowHistory,
  current: GraphSnap,
): { snap: GraphSnap; history: FlowHistory } | null {
  const snap = history.past[history.past.length - 1];
  if (!snap) return null;
  return {
    snap,
    history: {
      ...history,
      past: history.past.slice(0, -1),
      future: [...history.future, current].slice(-HISTORY_LIMIT),
      lastKey: undefined,
      lastAt: undefined,
      gesture: undefined,
    },
  };
}

/** Moves one step forward. Null when there is nothing to redo. */
export function redoStep(
  history: FlowHistory,
  current: GraphSnap,
): { snap: GraphSnap; history: FlowHistory } | null {
  const snap = history.future[history.future.length - 1];
  if (!snap) return null;
  return {
    snap,
    history: {
      ...history,
      past: [...history.past, current].slice(-HISTORY_LIMIT),
      future: history.future.slice(0, -1),
      lastKey: undefined,
      lastAt: undefined,
      gesture: undefined,
    },
  };
}

/** True when the graph is the saved one. Compares array references, which is cheap and exact. */
export function isAtSaved(snap: GraphSnap, saved: GraphSnap | undefined): boolean {
  if (!saved) return false;
  return (
    snap.nodes === saved.nodes &&
    snap.edges === saved.edges &&
    (snap.callbackHost ?? null) === (saved.callbackHost ?? null)
  );
}

/** Keeps only ids that still exist. Returns the same set when nothing changed. */
export function pruneSelection(
  selected: ReadonlySet<string>,
  liveIds: ReadonlySet<string>,
): ReadonlySet<string> {
  const kept = [...selected].filter((id) => liveIds.has(id));
  return kept.length === selected.size ? selected : new Set(kept);
}
