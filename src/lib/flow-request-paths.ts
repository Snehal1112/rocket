import type { GraphSnap } from '@/lib/flow-history';
import type { FlowNode } from '@/lib/tauri-api';
import type { FlowTab } from '@/types/pane-types';

function rewritePath(path: string, from: string, to: string): string | null {
  if (path === from) return to;
  if (path.startsWith(`${from}/`)) return `${to}${path.slice(from.length)}`;
  return null;
}

/**
 * Rewrites saved request paths in a flow tab, its nodes and every history
 * snapshot. Arrays that were one array stay one array. Returns the same tab
 * when nothing changes.
 */
export function rewriteTabPaths(tab: FlowTab, from: string, to: string): FlowTab {
  const base = from.endsWith('/') ? from.slice(0, -1) : from;
  if (base === '') return tab;
  const cache = new Map<FlowNode[], FlowNode[]>();

  const mapNode = (node: FlowNode): FlowNode => {
    const kind = node.kind;
    if (kind.kind !== 'Request' || kind.source.type !== 'Saved') return node;
    const next = rewritePath(kind.source.requestPath, base, to);
    if (next === null) return node;
    return { ...node, kind: { ...kind, source: { ...kind.source, requestPath: next } } };
  };

  const mapNodes = (nodes: FlowNode[]): FlowNode[] => {
    const hit = cache.get(nodes);
    if (hit) return hit;
    const mapped = nodes.map(mapNode);
    const result = mapped.some((n, i) => n !== nodes[i]) ? mapped : nodes;
    cache.set(nodes, result);
    return result;
  };

  const mapSnap = (snap: GraphSnap): GraphSnap => {
    const nodes = mapNodes(snap.nodes);
    return nodes === snap.nodes ? snap : { ...snap, nodes };
  };

  const mapSnaps = (snaps: GraphSnap[]): GraphSnap[] => {
    const mapped = snaps.map(mapSnap);
    return mapped.some((s, i) => s !== snaps[i]) ? mapped : snaps;
  };

  const nodes = mapNodes(tab.nodes);
  const h = tab.history;
  const history = h
    ? {
        ...h,
        past: mapSnaps(h.past),
        future: mapSnaps(h.future),
        saved: h.saved ? mapSnap(h.saved) : h.saved,
      }
    : h;
  const historyChanged =
    !!h &&
    !!history &&
    (history.past !== h.past || history.future !== h.future || history.saved !== h.saved);
  if (nodes === tab.nodes && !historyChanged) return tab;
  return { ...tab, nodes, history: historyChanged ? history : h };
}
