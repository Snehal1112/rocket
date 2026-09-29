import {
  Background,
  BackgroundVariant,
  type Connection,
  Controls,
  type Edge,
  type EdgeChange,
  type Node,
  type NodeChange,
  Panel,
  ReactFlow,
  ReactFlowProvider,
  SelectionMode,
  useReactFlow,
} from '@xyflow/react';
import { useMemo, useRef, useState } from 'react';
import { toast } from 'sonner';
import '@xyflow/react/dist/style.css';
import { decodeFlowRequestDragPayload } from '@/lib/flow-drag';
import { RESULT_HANDLE } from '@/lib/flow-handles';
import { type ConnectionLike, isValidFlowConnection } from '@/lib/flow-wiring';
import type { FlowEdge, FlowNode, FlowNodeKind, FlowNodeStatus } from '@/lib/tauri-api';
import type { FlowNodeDetail } from '@/types/pane-types';
import { edgeRunState, exitLabel } from './flowExits';
import { type FlowNodeActions, FlowNodeActionsContext } from './nodes/FlowNodeActionsContext';
import { IfNode } from './nodes/IfNode';
import { InputNode } from './nodes/InputNode';
import { OutputNode } from './nodes/OutputNode';
import { RequestNode } from './nodes/RequestNode';
import { SwitchNode } from './nodes/SwitchNode';

const nodeTypes = {
  Request: RequestNode,
  Input: InputNode,
  Output: OutputNode,
  If: IfNode,
  Switch: SwitchNode,
};

export interface FlowCanvasProps {
  nodes: FlowNode[];
  edges: FlowEdge[];
  nodeStatus: Record<string, FlowNodeStatus>;
  onNodesChange: (nodes: FlowNode[]) => void;
  onEdgesChange: (edges: FlowEdge[]) => void;
  onConnect: (connection: Connection) => void;
  // Called when a wire is double-clicked, to edit its script.
  onEdgeEdit?: (edgeId: string) => void;
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
  // Inline edits from routing nodes (If condition, Switch value and cases).
  onNodeKindChange?: (nodeId: string, kind: FlowNodeKind) => void;
  // Removes a Switch case together with the edges leaving its exit.
  onRemoveSwitchCase?: (nodeId: string, caseId: string) => void;
  // Controlled node selection. Without these props the canvas keeps its own
  // selection, as before. FlowPane passes them to drive the properties panel.
  selectedNodeIds?: ReadonlySet<string>;
  onSelectedNodeIdsChange?: (ids: ReadonlySet<string>) => void;
  // Called when a node's menu button opens its properties.
  onOpenProperties?: (nodeId: string) => void;
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

// Targets inside a node that own their focus: `.nokey` wrappers around
// inline fields and buttons, plus any plain editable element.
const EDITABLE_TARGET = '.nokey, input, textarea, [contenteditable]';

const CYCLE_EDGE_STYLE = { stroke: '#ef4444', strokeWidth: 2 };
const TAKEN_EDGE_STYLE = { stroke: '#22c55e', strokeWidth: 2 };
const NOT_TAKEN_EDGE_STYLE = { opacity: 0.35, strokeDasharray: '4 4' };

// The source handle is the edge's exit (absent means `result`). The target
// handle is the first segment of `targetField`, so
// "headers[Authorization].value" lands on the single `headers` handle.
// Routing exits get a label, and after a run their taken/not-taken state.
// A validation (cycle) highlight wins over run styling.
export function toRfEdges(
  edges: FlowEdge[],
  nodes: FlowNode[],
  nodeStatus: Record<string, FlowNodeStatus>,
  selectedIds: ReadonlySet<string>,
  nodeDetail?: FlowCanvasProps['nodeDetail'],
  cycleEdgeIds?: string[],
): Edge[] {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  return edges.map((e) => {
    const source = byId.get(e.sourceNodeId);
    const handle = e.sourceHandle ?? RESULT_HANDLE;
    const run = edgeRunState(
      e,
      source,
      nodeStatus[e.sourceNodeId],
      nodeDetail?.[e.sourceNodeId]?.branch,
    );
    const isCycle = cycleEdgeIds?.includes(e.id) ?? false;
    return {
      id: e.id,
      source: e.sourceNodeId,
      sourceHandle: handle,
      target: e.targetNodeId,
      targetHandle: e.targetField.split('[')[0],
      selected: selectedIds.has(e.id),
      label: source ? exitLabel(source.kind, handle) : undefined,
      // nopan keeps a double-click on a wire from also zooming the canvas.
      className: run === 'neutral' ? 'nopan' : `nopan flow-edge-${run}`,
      style: isCycle
        ? CYCLE_EDGE_STYLE
        : run === 'taken'
          ? TAKEN_EDGE_STYLE
          : run === 'not-taken'
            ? NOT_TAKEN_EDGE_STYLE
            : undefined,
    };
  });
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
  onEdgeEdit,
  onAddNode,
  flowCollectionName,
  nodeDetail,
  cycleNodeIds,
  cycleEdgeIds,
  onNodeKindChange,
  onRemoveSwitchCase,
  selectedNodeIds: selectedNodeIdsProp,
  onSelectedNodeIdsChange,
  onOpenProperties,
}: FlowCanvasProps) {
  const { screenToFlowPosition } = useReactFlow();
  const [localSelection, setLocalSelection] = useState<ReadonlySet<string>>(() => new Set());
  const selectedNodeIds = selectedNodeIdsProp ?? localSelection;
  // Reports a new selection to the owner, or keeps it locally when uncontrolled.
  const selectNodes = (next: ReadonlySet<string>) => {
    if (next === selectedNodeIds) return;
    if (onSelectedNodeIdsChange) onSelectedNodeIdsChange(next);
    else setLocalSelection(next);
  };
  // Node actions are memoised, so they call the latest selectNodes through a ref.
  const selectNodesRef = useRef(selectNodes);
  selectNodesRef.current = selectNodes;
  const onOpenPropertiesRef = useRef(onOpenProperties);
  onOpenPropertiesRef.current = onOpenProperties;
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
  // React Flow reports every click inside a node, including clicks on its
  // inline fields and buttons. Moving focus there would pull it out of the
  // field, so editable targets keep their focus. The selector matches the
  // one React Flow uses to ignore key presses.
  const handleNodeClick = (e: React.MouseEvent) => {
    if (e.target instanceof Element && e.target.closest(EDITABLE_TARGET)) return;
    focusPane();
  };

  const rfNodes = useMemo(
    () =>
      toRfNodes(nodes, nodeStatus, selectedNodeIds, measuredRef.current, nodeDetail, cycleNodeIds),
    [nodes, nodeStatus, selectedNodeIds, nodeDetail, cycleNodeIds],
  );
  const rfEdges = useMemo(
    () => toRfEdges(edges, nodes, nodeStatus, selectedEdgeIds, nodeDetail, cycleEdgeIds),
    [edges, nodes, nodeStatus, selectedEdgeIds, nodeDetail, cycleEdgeIds],
  );
  const nodeActions = useMemo<FlowNodeActions>(
    () => ({
      updateNodeKind: (nodeId, kind) => onNodeKindChange?.(nodeId, kind),
      removeSwitchCase: (nodeId, caseId) => onRemoveSwitchCase?.(nodeId, caseId),
      openProperties: (nodeId) => {
        selectNodesRef.current(new Set([nodeId]));
        onOpenPropertiesRef.current?.(nodeId);
      },
    }),
    [onNodeKindChange, onRemoveSwitchCase],
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
    selectNodes(nextSelection(selectedNodeIds, changes));
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

  // Ctrl+A (Cmd+A on macOS) selects every node, unless a field owns the keys.
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key.toLowerCase() !== 'a' || !(e.ctrlKey || e.metaKey)) return;
    if (e.target instanceof Element && e.target.closest(EDITABLE_TARGET)) return;
    e.preventDefault();
    selectNodes(new Set(nodes.map((n) => n.id)));
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
      onKeyDown={handleKeyDown}
    >
      <FlowNodeActionsContext.Provider value={nodeActions}>
        <ReactFlow
          nodes={rfNodes}
          edges={rfEdges}
          nodeTypes={nodeTypes}
          onNodesChange={handleNodesChange}
          onEdgesChange={handleEdgesChange}
          onConnect={onConnect}
          isValidConnection={isValidConnection}
          onNodeClick={handleNodeClick}
          onEdgeClick={focusPane}
          onEdgeDoubleClick={(_, edge) => {
            // Editing a wire must not leave it selected, or a later Backspace deletes it.
            setSelectedEdgeIds((prev) => {
              if (!prev.has(edge.id)) return prev;
              const next = new Set(prev);
              next.delete(edge.id);
              return next;
            });
            onEdgeEdit?.(edge.id);
          }}
          onPaneClick={focusPane}
          // A box-select does not fire a pane click, so refocus the pane here.
          onSelectionEnd={focusPane}
          // Tolerate small jitter so a click is not turned into an empty selection.
          paneClickDistance={4}
          // Left-drag draws a selection box. Middle and right drag pan instead.
          selectionOnDrag
          panOnDrag={[1, 2]}
          selectionMode={SelectionMode.Partial}
          panOnScroll
          onPaneContextMenu={(e) => e.preventDefault()}
          deleteKeyCode={['Backspace', 'Delete']}
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
          <Panel position='bottom-left' className='pointer-events-none ml-14 mb-3'>
            <span className='text-[11px] text-muted-foreground/70'>
              Drag to select · Ctrl+A select all · Right-drag or scroll to pan · Ctrl+scroll to zoom
            </span>
          </Panel>
        </ReactFlow>
      </FlowNodeActionsContext.Provider>
    </div>
  );
}
