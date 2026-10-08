import { caseHandle, caseIdFromHandle } from '@/lib/flow-handles';
import { newEntityId } from '@/lib/flow-ids';
import type { FlowEdge, FlowNode, FlowNodeKind } from '@/lib/tauri-api';

// Pixels a pasted node moves right and down from its original, per paste.
export const PASTE_OFFSET = 40;

export interface FlowClip {
  // The collection the nodes came from. Saved requests only resolve inside it.
  collection: string | null;
  nodes: FlowNode[];
  edges: FlowEdge[];
}

export interface PasteResult {
  nodes: FlowNode[];
  edges: FlowEdge[];
  // Short messages for the user, such as a renamed callback.
  notices: string[];
}

/**
 * Builds a clip from the selected nodes and the wires between them. A wire is
 * kept only when both of its ends are selected. The clip is a deep copy.
 * Returns null when no selected node exists.
 */
export function copySelection(
  nodes: FlowNode[],
  edges: FlowEdge[],
  ids: ReadonlySet<string>,
  collection: string | null,
): FlowClip | null {
  const picked = nodes.filter((n) => ids.has(n.id));
  if (picked.length === 0) return null;
  const kept = new Set(picked.map((n) => n.id));
  return structuredClone({
    collection,
    nodes: picked,
    edges: edges.filter((e) => kept.has(e.sourceNodeId) && kept.has(e.targetNodeId)),
  });
}

/** `name`, then `name_2`, `name_3`, ... whichever is free first. */
export function uniqueCallbackName(name: string, taken: ReadonlySet<string>): string {
  if (!taken.has(name)) return name;
  let i = 2;
  while (taken.has(`${name}_${i}`)) i += 1;
  return `${name}_${i}`;
}

/** A reason the clip cannot go into this collection, or null when it can. */
export function canPasteInto(clip: FlowClip, collection: string | null): string | null {
  const hasSaved = clip.nodes.some(
    (n) => n.kind.kind === 'Request' && n.kind.source.type === 'Saved',
  );
  if (hasSaved && clip.collection !== collection) {
    return `Cannot paste saved requests from "${clip.collection ?? 'another collection'}" into this flow's collection.`;
  }
  return null;
}

/**
 * Makes new nodes and wires from a clip. Every id is new, positions move by
 * `PASTE_OFFSET * step`, and the per-kind rules apply: an Auth node stops
 * applying to inherited auth (V13), a Switch gets new case ids and its wires
 * follow them, and a Wait for callback gets a free name (V10). Inputs are never changed.
 */
export function instantiatePaste(clip: FlowClip, existingNodes: FlowNode[], step = 1): PasteResult {
  const fresh = structuredClone({ nodes: clip.nodes, edges: clip.edges });
  const idMap = new Map<string, string>();
  // Old node id, then old case id, to new case id.
  const caseMaps = new Map<string, Map<string, string>>();
  const takenNames = new Set(
    existingNodes.flatMap((n) => (n.kind.kind === 'WaitForCallback' ? [n.kind.name] : [])),
  );
  const notices: string[] = [];
  const offset = PASTE_OFFSET * step;

  const nodes = fresh.nodes.map((node): FlowNode => {
    const id = newEntityId();
    idMap.set(node.id, id);
    const kind = freshKind(node, takenNames, caseMaps, notices);
    return {
      ...node,
      id,
      kind,
      position: { x: node.position.x + offset, y: node.position.y + offset },
    };
  });

  const edges = fresh.edges.flatMap((edge): FlowEdge[] => {
    const sourceNodeId = idMap.get(edge.sourceNodeId);
    const targetNodeId = idMap.get(edge.targetNodeId);
    if (!sourceNodeId || !targetNodeId) return [];
    const pasted: FlowEdge = { ...edge, id: newEntityId(), sourceNodeId, targetNodeId };
    const oldCase = edge.sourceHandle ? caseIdFromHandle(edge.sourceHandle) : null;
    if (oldCase !== null) {
      const newCase = caseMaps.get(edge.sourceNodeId)?.get(oldCase);
      // A wire from a case the node no longer has cannot be kept.
      if (!newCase) return [];
      pasted.sourceHandle = caseHandle(newCase);
    }
    return [pasted];
  });

  return { nodes, edges, notices };
}

// Applies the per-kind paste rules to one cloned node.
function freshKind(
  node: FlowNode,
  takenNames: Set<string>,
  caseMaps: Map<string, Map<string, string>>,
  notices: string[],
): FlowNodeKind {
  const kind = node.kind;
  if (kind.kind === 'Auth') {
    if (kind.applyToInherit) {
      notices.push(
        'The pasted Auth node does not apply to inherited auth. Only one Auth node in a flow can.',
      );
    }
    return { ...kind, applyToInherit: false };
  }
  if (kind.kind === 'Switch') {
    const map = new Map<string, string>();
    const cases = kind.cases.map((c) => {
      const id = newEntityId();
      map.set(c.id, id);
      return { ...c, id };
    });
    caseMaps.set(node.id, map);
    return { ...kind, cases };
  }
  if (kind.kind === 'WaitForCallback') {
    const name = uniqueCallbackName(kind.name, takenNames);
    takenNames.add(name);
    if (name !== kind.name) {
      notices.push(
        `Renamed callback "${kind.name}" to "${name}". A pasted request that sends {{callback.${kind.name}}} still points at the original and may fail validation when you save.`,
      );
    }
    return { ...kind, name };
  }
  return kind;
}

// The clipboard lives in memory only. It survives tab switches but not a reload.
let clipboard: FlowClip | null = null;
let pasteCount = 0;

export function setFlowClipboard(clip: FlowClip): void {
  clipboard = clip;
  pasteCount = 0;
}

export function getFlowClipboard(): FlowClip | null {
  return clipboard;
}

/** The step for the next paste of the current clip: 1, then 2, then 3. */
export function nextPasteStep(): number {
  pasteCount += 1;
  return pasteCount;
}

export function clearFlowClipboard(): void {
  clipboard = null;
  pasteCount = 0;
}
