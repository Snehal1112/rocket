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
import { useMemo } from 'react';
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

// Maps our backend-shaped FlowNode/FlowEdge into React Flow's own Node/Edge
// shape. `type` selects the nodeTypes entry above; everything else our
// custom node components need travels in `data`.
function toRfNodes(nodes: FlowNode[], nodeStatus: Record<string, FlowNodeStatus>): Node[] {
  return nodes.map((n) => ({
    id: n.id,
    type: n.kind.kind,
    position: n.position,
    data: { kind: n.kind, status: nodeStatus[n.id] ?? 'idle' },
  }));
}

function toRfEdges(edges: FlowEdge[]): Edge[] {
  return edges.map((e) => ({
    id: e.id,
    source: e.sourceNodeId,
    target: e.targetNodeId,
    targetHandle: e.targetField.split('[')[0],
  }));
}

export function FlowCanvas({
  nodes,
  edges,
  nodeStatus,
  onNodesChange,
  onEdgesChange,
  onConnect,
}: FlowCanvasProps) {
  const rfNodes = useMemo(() => toRfNodes(nodes, nodeStatus), [nodes, nodeStatus]);
  const rfEdges = useMemo(() => toRfEdges(edges), [edges]);

  // React Flow's onNodesChange/onEdgesChange report deltas (position drags,
  // deletions, selection). We only need to persist deletions and position
  // moves back into FlowTab state for v1 — dragging a node updates its
  // `position`; deleting a node also drops any edge that referenced it, so
  // Plan 10's onConnect additions and this handler never leave a dangling
  // edge behind.
  const handleNodesChange = (changes: NodeChange[]) => {
    let next = nodes;
    for (const change of changes) {
      if (change.type === 'position' && change.position) {
        const position = change.position;
        next = next.map((n) => (n.id === change.id ? { ...n, position } : n));
      } else if (change.type === 'remove') {
        next = next.filter((n) => n.id !== change.id);
      }
    }
    if (next !== nodes) onNodesChange(next);
  };

  const handleEdgesChange = (changes: EdgeChange[]) => {
    let next = edges;
    for (const change of changes) {
      if (change.type === 'remove') {
        next = next.filter((e) => e.id !== change.id);
      }
    }
    if (next !== edges) onEdgesChange(next);
  };

  return (
    <ReactFlowProvider>
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
    </ReactFlowProvider>
  );
}
