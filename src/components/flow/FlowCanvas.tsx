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
} from '@xyflow/react';
import { useMemo, useRef, useState } from 'react';
import '@xyflow/react/dist/style.css';
import type { FlowEdge, FlowNode, FlowNodeStatus } from '@/lib/tauri-api';
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
): Node[] {
  return nodes.map((n) => ({
    id: n.id,
    type: n.kind.kind,
    position: n.position,
    data: { kind: n.kind, status: nodeStatus[n.id] ?? 'idle' },
    selected: selectedIds.has(n.id),
    measured: measured.get(n.id),
  }));
}

// Every node type has one source handle, `result`. The target handle is the
// first segment of `targetField`, so "headers[Authorization].value" lands on
// the single `headers` handle.
function toRfEdges(edges: FlowEdge[], selectedIds: ReadonlySet<string>): Edge[] {
  return edges.map((e) => ({
    id: e.id,
    source: e.sourceNodeId,
    sourceHandle: 'result',
    target: e.targetNodeId,
    targetHandle: e.targetField.split('[')[0],
    selected: selectedIds.has(e.id),
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
}: FlowCanvasProps) {
  const [selectedNodeIds, setSelectedNodeIds] = useState<ReadonlySet<string>>(() => new Set());
  const [selectedEdgeIds, setSelectedEdgeIds] = useState<ReadonlySet<string>>(() => new Set());
  // A ref, not state: React Flow already holds the new size internally, so
  // recording it must not trigger a re-render.
  const measuredRef = useRef(new Map<string, Measured>());

  const rfNodes = useMemo(
    () => toRfNodes(nodes, nodeStatus, selectedNodeIds, measuredRef.current),
    [nodes, nodeStatus, selectedNodeIds],
  );
  const rfEdges = useMemo(() => toRfEdges(edges, selectedEdgeIds), [edges, selectedEdgeIds]);

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

  return (
    <div data-testid='flow-canvas' className='h-full w-full'>
      <ReactFlow
        nodes={rfNodes}
        edges={rfEdges}
        nodeTypes={nodeTypes}
        onNodesChange={handleNodesChange}
        onEdgesChange={handleEdgesChange}
        onConnect={onConnect}
        fitView
      >
        <Background variant={BackgroundVariant.Dots} gap={16} size={1} />
        <Controls />
      </ReactFlow>
    </div>
  );
}
