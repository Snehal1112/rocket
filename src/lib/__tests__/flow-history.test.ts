import { describe, expect, it } from 'vitest';
import type { FlowNode } from '@/lib/tauri-api';
import {
  DEFAULT_COALESCE_MS,
  type FlowHistory,
  type GraphSnap,
  HISTORY_LIMIT,
  isAtSaved,
  pruneSelection,
  recordEdit,
  redoStep,
  snapOf,
  undoStep,
} from '../flow-history';

const node = (id: string, x = 0): FlowNode => ({
  id,
  kind: { kind: 'Output', label: id },
  position: { x, y: 0 },
});

const snap = (...ids: string[]): GraphSnap => ({ nodes: ids.map((id) => node(id)), edges: [] });

describe('recordEdit', () => {
  it('pushes the previous snapshot and clears the future', () => {
    const start: FlowHistory = { past: [], future: [snap('z')] };
    const next = recordEdit(start, snap('a'), undefined, 0);
    expect(next.past).toEqual([snap('a')]);
    expect(next.future).toEqual([]);
  });

  it('creates a history when the tab has none', () => {
    const next = recordEdit(undefined, snap('a'), undefined, 0);
    expect(next.past).toHaveLength(1);
  });

  it('keeps at most HISTORY_LIMIT steps and drops the oldest', () => {
    let h: FlowHistory | undefined;
    for (let i = 0; i < HISTORY_LIMIT + 5; i += 1) {
      h = recordEdit(h, { nodes: [node(`n${i}`)], edges: [] }, undefined, i);
    }
    expect(h?.past).toHaveLength(HISTORY_LIMIT);
    expect(h?.past[0].nodes[0].id).toBe('n5');
    expect(h?.past[HISTORY_LIMIT - 1].nodes[0].id).toBe(`n${HISTORY_LIMIT + 4}`);
  });

  it('coalesces writes with the same key inside the window', () => {
    const first = recordEdit(undefined, snap('a'), { coalesceKey: 'k' }, 1000);
    const second = recordEdit(first, snap('b'), { coalesceKey: 'k' }, 1000 + DEFAULT_COALESCE_MS);
    expect(second.past).toEqual([snap('a')]);
  });

  it('slides the window on every coalesced write', () => {
    let h = recordEdit(undefined, snap('a'), { coalesceKey: 'k' }, 0);
    for (let t = 500; t <= 2500; t += 500) {
      h = recordEdit(h, snap('b'), { coalesceKey: 'k' }, t);
    }
    expect(h.past).toHaveLength(1);
  });

  it('starts a new step when the window has passed', () => {
    const first = recordEdit(undefined, snap('a'), { coalesceKey: 'k' }, 0);
    const second = recordEdit(first, snap('b'), { coalesceKey: 'k' }, DEFAULT_COALESCE_MS + 1);
    expect(second.past).toHaveLength(2);
  });

  it('honours a custom coalesceMs', () => {
    const first = recordEdit(undefined, snap('a'), { coalesceKey: 'k', coalesceMs: 50 }, 0);
    const inside = recordEdit(first, snap('b'), { coalesceKey: 'k', coalesceMs: 50 }, 50);
    const outside = recordEdit(first, snap('b'), { coalesceKey: 'k', coalesceMs: 50 }, 51);
    expect(inside.past).toHaveLength(1);
    expect(outside.past).toHaveLength(2);
  });

  it('starts a new step for a different key', () => {
    const first = recordEdit(undefined, snap('a'), { coalesceKey: 'k1' }, 0);
    const second = recordEdit(first, snap('b'), { coalesceKey: 'k2' }, 10);
    expect(second.past).toHaveLength(2);
  });

  it('does not coalesce across an unkeyed write', () => {
    const first = recordEdit(undefined, snap('a'), { coalesceKey: 'k' }, 0);
    const plain = recordEdit(first, snap('b'), undefined, 10);
    const third = recordEdit(plain, snap('c'), { coalesceKey: 'k' }, 20);
    expect(third.past).toHaveLength(3);
  });

  it('records a whole gesture as one step, on its first write', () => {
    const armed: FlowHistory = { past: [], future: [], gesture: 'armed' };
    let h = recordEdit(armed, snap('a'), { gesture: true }, 0);
    h = recordEdit(h, snap('b'), { gesture: true }, 1);
    h = recordEdit(h, snap('c'), { gesture: true }, 2);
    expect(h.past).toEqual([snap('a')]);
    expect(h.gesture).toBe('recorded');
  });

  it('records the next gesture after the previous one was re-armed', () => {
    const armed: FlowHistory = { past: [snap('old')], future: [], gesture: 'armed' };
    const h = recordEdit(armed, snap('a'), { gesture: true }, 0);
    expect(h.past).toEqual([snap('old'), snap('a')]);
  });
});

describe('undoStep and redoStep', () => {
  it('swaps the current snapshot with the last past one', () => {
    const h: FlowHistory = { past: [snap('a'), snap('b')], future: [] };
    const undone = undoStep(h, snap('c'));
    expect(undone?.snap).toEqual(snap('b'));
    expect(undone?.history.past).toEqual([snap('a')]);
    expect(undone?.history.future).toEqual([snap('c')]);
  });

  it('redo reverses undo', () => {
    const h: FlowHistory = { past: [snap('a')], future: [] };
    const undone = undoStep(h, snap('b'));
    if (!undone) throw new Error('Expected an undo step');
    const redone = redoStep(undone.history, undone.snap);
    expect(redone?.snap).toEqual(snap('b'));
    expect(redone?.history.past).toEqual([snap('a')]);
    expect(redone?.history.future).toEqual([]);
  });

  it('returns null when there is nothing to do', () => {
    expect(undoStep({ past: [], future: [] }, snap('a'))).toBeNull();
    expect(redoStep({ past: [], future: [] }, snap('a'))).toBeNull();
  });

  it('resets the coalesce key and the gesture', () => {
    const h: FlowHistory = {
      past: [snap('a')],
      future: [],
      lastKey: 'k',
      lastAt: 5,
      gesture: 'recorded',
    };
    const undone = undoStep(h, snap('b'));
    expect(undone?.history.lastKey).toBeUndefined();
    expect(undone?.history.gesture).toBeUndefined();
  });

  it('keeps the saved snapshot', () => {
    const saved = snap('s');
    const undone = undoStep({ past: [snap('a')], future: [], saved }, snap('b'));
    expect(undone?.history.saved).toBe(saved);
  });
});

describe('isAtSaved', () => {
  it('is true only for the same node and edge arrays', () => {
    const saved = snap('a');
    expect(isAtSaved({ ...saved }, saved)).toBe(true);
    expect(isAtSaved(snap('a'), saved)).toBe(false);
  });

  it('is false without a saved snapshot', () => {
    expect(isAtSaved(snap('a'), undefined)).toBe(false);
  });

  it('treats a null and an absent callback host as equal', () => {
    const saved: GraphSnap = { nodes: [], edges: [], callbackHost: null };
    expect(isAtSaved({ nodes: saved.nodes, edges: saved.edges }, saved)).toBe(true);
    expect(isAtSaved({ nodes: saved.nodes, edges: saved.edges, callbackHost: 'h' }, saved)).toBe(
      false,
    );
  });
});

describe('snapOf', () => {
  it('keeps the same array references', () => {
    const nodes = [node('a')];
    const edges: GraphSnap['edges'] = [];
    const s = snapOf({ nodes, edges, callbackHost: 'h' });
    expect(s.nodes).toBe(nodes);
    expect(s.edges).toBe(edges);
    expect(s.callbackHost).toBe('h');
  });
});

describe('pruneSelection', () => {
  it('returns the same set when every id still exists', () => {
    const selected = new Set(['a', 'b']);
    expect(pruneSelection(selected, new Set(['a', 'b', 'c']))).toBe(selected);
  });

  it('drops ids that no longer exist', () => {
    expect(pruneSelection(new Set(['a', 'b']), new Set(['a']))).toEqual(new Set(['a']));
  });
});
