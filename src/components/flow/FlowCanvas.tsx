import {
  Background,
  BackgroundVariant,
  type Connection,
  Controls,
  type Edge,
  type EdgeChange,
  type Node,
  type NodeChange,
  ReactFlow,
  ReactFlowProvider,
  useReactFlow,
} from '@xyflow/react';
import { useMemo, useRef, useState } from 'react';
import { toast } from 'sonner';
import '@xyflow/react/dist/style.css';
import { decodeFlowRequestDragPayload } from '@/lib/flow-drag';
import { RESULT_HANDLE } from '@/lib/flow-handles';
import { type ConnectionLike, isValidFlowConnection } from '@/lib/flow-wiring';
import type { FlowEdge, FlowNode, FlowNodeStatus } from '@/lib/tauri-api';
import type { FlowNodeDetail } from '@/types/pane-types';
import { InputNode } from './nodes/InputNode';
import { OutputNode } from './nodes/OutputNode';
import { RequestNode } from './nodes/RequestNode';

const nodeTypes = {
  Request: RequestNode,
  Input: InputNode,
  Output: OutputNode,
};

export interface FlowCanvasProps {
  nodes: FlowNode[];
  edges: FlowEdge[];
  nodeStatus: Record<string, FlowNodeStatus>;
  onNodesChange: (nodes: FlowNode[]) => void;
  onEdgesChange: (edges: FlowEdge[]) => void;
  onConnect: (connection: Connection) => void;
  onAddNode?: (node: FlowNode) => void;
  // The collection the current flow belongs to. A dropped request from a
  // different collection is rejected: `RequestSource.Saved` only carries a
  // `requestPath`, resolved at run time against the flow's own collection
  // (see rocket-app/flow_execution_service.rs), so a cross-collection drop
  // would silently resolve to the wrong file (or fail to resolve at all).
  flowCollectionName?: string | null;
  // Per-node status-code/timing/error, keyed by node id. Populated by a run.
  nodeDetail?: Record<string, FlowNodeDetail>;
  // Node ids named in a save validation error, such as a cycle.
  cycleNodeIds?: string[];
  // Edge ids named in a save validation error, such as a cycle.
  cycleEdgeIds?: string[];
}

type Measured = { width: number; height: number };

// Maps our backend-shaped FlowNode/FlowEdge into React Flow's own Node/Edge
// shape. `type` selects the nodeTypes entry above; everything else our
// custom node components need travels in `data`.
//
// Every call builds new node objects, so React Flow treats each one as a
// fresh node. Passing back the last measured size keeps the node visible
// and its handle positions intact instead of re-measuring from scratch.
// Selection lives in canvas-local state, because it is not persisted.
function toRfNodes(
  nodes: FlowNode[],
  nodeStatus: Record<string, FlowNodeStatus>,
  selectedIds: ReadonlySet<string>,
  measured: ReadonlyMap<string, Measured>,
  nodeDetail?: Record<string, FlowNodeDetail>,
  cycleNodeIds?: string[],
): Node[] {
  return nodes.map((n) => ({
    id: n.id,
    type: n.kind.kind,
    position: n.position,
    data: {
      kind: n.kind,
      status: nodeStatus[n.id] ?? 'idle',
      ...nodeDetail?.[n.id],
      hasCycleError: cycleNodeIds?.includes(n.id) ?? false,
    },
    selected: selectedIds.has(n.id),
    measured: measured.get(n.id),
  }));
}

// An edge leaves the source exit named by `sourceHandle`. It is absent for
// the default `result` exit. The target handle is the first segment of
// `targetField`, so "headers[Authorization].value" lands on the single
// `headers` handle.
export function toRfEdges(
  edges: FlowEdge[],
  selectedIds: ReadonlySet<string>,
  cycleEdgeIds?: string[],
): Edge[] {
  return edges.map((e) => ({
    id: e.id,
    source: e.sourceNodeId,
    sourceHandle: e.sourceHandle ?? RESULT_HANDLE,
    target: e.targetNodeId,
    targetHandle: e.targetField.split('[')[0],
    selected: selectedIds.has(e.id),
    style: cycleEdgeIds?.includes(e.id) ? { stroke: '#ef4444', strokeWidth: 2 } : undefined,
  }));
}

// Applies select and remove changes to a selection set. It returns the same
// set when nothing changed, so React skips the re-render.
function nextSelection(
  prev: ReadonlySet<string>,
  changes: Array<NodeChange | EdgeChange>,
): ReadonlySet<string> {
  const next = new Set(prev);
  for (const change of changes) {
    if (change.type === 'select') {
      if (change.selected) next.add(change.id);
      else next.delete(change.id);
    } else if (change.type === 'remove') {
      next.delete(change.id);
    }
  }
  const same = next.size === prev.size && [...next].every((id) => prev.has(id));
  return same ? prev : next;
}

// `ReactFlowProvider` wraps the canvas exactly once, here. Code that needs
// `useReactFlow()` (for example drop handling) belongs in FlowCanvasInner.
export function FlowCanvas(props: FlowCanvasProps) {
  return (
    <ReactFlowProvider>
      <FlowCanvasInner {...props} />
    </ReactFlowProvider>
  );
}

function FlowCanvasInner({
  nodes,
  edges,
  nodeStatus,
  onNodesChange,
  onEdgesChange,
  onConnect,
  onAddNode,
  flowCollectionName,
  nodeDetail,
  cycleNodeIds,
  cycleEdgeIds,
}: FlowCanvasProps) {
  const { screenToFlowPosition } = useReactFlow();
  const [selectedNodeIds, setSelectedNodeIds] = useState<ReadonlySet<string>>(() => new Set());
  const [selectedEdgeIds, setSelectedEdgeIds] = useState<ReadonlySet<string>>(() => new Set());
  // A ref, not state: React Flow already holds the new size internally, so
  // recording it must not trigger a re-render.
  const measuredRef = useRef(new Map<string, Measured>());
  // React Flow's delete-key handler no-ops while focus sits on an
  // input/textarea/contenteditable (@xyflow/react's isInputDOMNode check).
  // Clicking a node only flips its `selected` flag; it never moves DOM
  // focus. Chromium auto-focuses (and blurs the prior element for) any
  // clicked tabIndex element, so this never surfaces there, but WebKitGTK
  // (what Tauri runs on Linux) does not — leaving focus stuck in whatever
  // text field was last active and silently breaking Backspace/Delete.
  const paneRef = useRef<HTMLDivElement>(null);
  const focusPane = () => paneRef.current?.focus();

  const rfNodes = useMemo(
    () =>
      toRfNodes(nodes, nodeStatus, selectedNodeIds, measuredRef.current, nodeDetail, cycleNodeIds),
    [nodes, nodeStatus, selectedNodeIds, nodeDetail, cycleNodeIds],
  );
  const rfEdges = useMemo(
    () => toRfEdges(edges, selectedEdgeIds, cycleEdgeIds),
    [edges, selectedEdgeIds, cycleEdgeIds],
  );

  // Refuses wires the backend would reject, while the user is still dragging.
  const isValidConnection = (connection: ConnectionLike) =>
    isValidFlowConnection(connection, nodes, edges);

  // React Flow's onNodesChange/onEdgesChange report deltas. Positions and
  // deletions are persisted into FlowTab state. Selection and measured
  // sizes stay local to the canvas. Deleting a node makes React Flow emit a
  // remove change for each connected edge too, so no edge is left dangling.
  const handleNodesChange = (changes: NodeChange[]) => {
    let next = nodes;
    for (const change of changes) {
      if (change.type === 'position' && change.position) {
        const position = change.position;
        next = next.map((n) => (n.id === change.id ? { ...n, position } : n));
      } else if (change.type === 'dimensions' && change.dimensions) {
        measuredRef.current.set(change.id, change.dimensions);
      } else if (change.type === 'remove') {
        measuredRef.current.delete(change.id);
        next = next.filter((n) => n.id !== change.id);
      }
    }
    setSelectedNodeIds((prev) => nextSelection(prev, changes));
    if (next !== nodes) onNodesChange(next);
  };

  const handleEdgesChange = (changes: EdgeChange[]) => {
    let next = edges;
    for (const change of changes) {
      if (change.type === 'remove') {
        next = next.filter((e) => e.id !== change.id);
      }
    }
    setSelectedEdgeIds((prev) => nextSelection(prev, changes));
    if (next !== edges) onEdgesChange(next);
  };

  // Drag-and-drop from the collection sidebar. A drop with no valid flow-drag
  // payload (e.g. a stray file drag) is a no-op.
  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'copy';
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    const payload = decodeFlowRequestDragPayload(e.dataTransfer);
    if (!payload) return;

    // A saved request node only stores `requestPath`, resolved against this
    // flow's own collection at run time. Dropping a request from a different
    // collection would silently misresolve, so reject it instead.
    if (flowCollectionName && payload.collection !== flowCollectionName) {
      toast.error(`Cannot add "${payload.name}": it belongs to a different collection.`);
      return;
    }

    const position = screenToFlowPosition({ x: e.clientX, y: e.clientY });
    onAddNode?.({
      id: crypto.randomUUID(),
      kind: {
        kind: 'Request',
        label: payload.name,
        source: { type: 'Saved', requestPath: payload.path },
      },
      position,
    });
  };

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: drop target for sidebar request drag-and-drop
    <div
      ref={paneRef}
      tabIndex={-1}
      data-testid='flow-canvas'
      className='h-full w-full outline-none'
      onDragOver={handleDragOver}
      onDrop={handleDrop}
    >
      <ReactFlow
        nodes={rfNodes}
        edges={rfEdges}
        nodeTypes={nodeTypes}
        onNodesChange={handleNodesChange}
        onEdgesChange={handleEdgesChange}
        onConnect={onConnect}
        isValidConnection={isValidConnection}
        onNodeClick={focusPane}
        onEdgeClick={focusPane}
        onPaneClick={focusPane}
        fitView
      >
        {/* The theme token keeps the dots visible on both light and dark canvases. */}
        <Background
          variant={BackgroundVariant.Dots}
          gap={16}
          size={1.5}
          color='hsl(var(--muted-foreground))'
        />
        <Controls />
      </ReactFlow>
    </div>
  );
}
