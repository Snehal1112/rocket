import type { Connection } from '@xyflow/react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '@/components/ui/resizable';
import { getActiveGlobalEnvName } from '@/lib/execute-request';
import { collectFlowAuthTokens } from '@/lib/flow-auth-preflight';
import {
  canPasteInto,
  copySelection,
  type FlowClip,
  getFlowClipboard,
  instantiatePaste,
  nextPasteStep,
  setFlowClipboard,
} from '@/lib/flow-clipboard';
import { removeSwitchCase, replaceNodeKind } from '@/lib/flow-graph-edits';
import { type FlowWriteOptions, pruneSelection, snapOf } from '@/lib/flow-history';
import { computeFlowIssues } from '@/lib/flow-issues';
import { buildRunRecord } from '@/lib/flow-run-history';
import type { FlowRunResult } from '@/lib/flow-run-result';
import { flowPayloadFromTab } from '@/lib/flow-save';
import {
  buildEdgeFromConnection,
  parseGraphErrorMessage,
  shouldPromptForExpression,
} from '@/lib/flow-wiring';
import { findTabInTree } from '@/lib/pane-utils';
import {
  type FlowEdge,
  type FlowNode,
  type FlowNodeKind,
  type FlowNodeStatus,
  saveFlow,
} from '@/lib/tauri-api';
import { useConsoleStore } from '@/stores/console-store';
import { useEnvStore } from '@/stores/env-store';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { CallbackHostSetting } from './CallbackHostSetting';
import { FlowCanvas } from './FlowCanvas';
import { FlowExportMenu } from './FlowExportMenu';
import { FlowHistoryButtons } from './FlowHistoryButtons';
import { FlowIssuesButton } from './FlowIssuesButton';
import { FlowPicker } from './FlowPicker';
import { FlowRunAnnouncer } from './FlowRunAnnouncer';
import { FlowSaveShortcut } from './FlowSaveShortcut';
import { FlowToolbar } from './FlowToolbar';
import { NodePalette } from './NodePalette';
import { NodePropertiesPanel, type PanelTab } from './properties/NodePropertiesPanel';
import { RunHistorySelect } from './RunHistorySelect';
import { RunResultStrip } from './RunResultStrip';
import { useClearRemovedAuthTokens } from './useClearRemovedAuthTokens';
import { ViewedRunBanner } from './ViewedRunBanner';
import { WireScriptDialog } from './WireScriptDialog';

// A new wire and the dialog that finishes it are one undo step, however long the dialog stays open.
const WIRE_STEP = (edgeId: string): FlowWriteOptions => ({
  coalesceKey: `wire:${edgeId}`,
  coalesceMs: Number.POSITIVE_INFINITY,
});

export function FlowPane({ tab, groupId }: { tab: FlowTab; groupId: string }) {
  const updateFlowNodes = usePaneStore((s) => s.updateFlowNodes);
  const updateFlowGraph = usePaneStore((s) => s.updateFlowGraph);
  const setFlowCallbackHost = usePaneStore((s) => s.setFlowCallbackHost);
  const updateFlowEdges = usePaneStore((s) => s.updateFlowEdges);
  const patchFlowNodeStatus = usePaneStore((s) => s.patchFlowNodeStatus);
  const patchFlowNodeProgress = usePaneStore((s) => s.patchFlowNodeProgress);
  const setFlowRunState = usePaneStore((s) => s.setFlowRunState);
  const setFlowPendingRun = usePaneStore((s) => s.setFlowPendingRun);
  const setFlowCallbackUrls = usePaneStore((s) => s.setFlowCallbackUrls);
  const setFlowRunResult = usePaneStore((s) => s.setFlowRunResult);
  const recordFlowRun = usePaneStore((s) => s.recordFlowRun);
  const setViewedFlowRun = usePaneStore((s) => s.setViewedFlowRun);
  const markClean = usePaneStore((s) => s.markClean);
  const undoFlow = usePaneStore((s) => s.undoFlow);
  const redoFlow = usePaneStore((s) => s.redoFlow);
  const beginFlowGesture = usePaneStore((s) => s.beginFlowGesture);
  const endFlowGesture = usePaneStore((s) => s.endFlowGesture);
  // There is no `activeEnvironmentName` anywhere. The active environment's
  // name is env-store's `activeEnvId` (it holds the name; see
  // src/lib/execute-request.ts, which passes it as environmentName).
  const activeEnvironmentName = useEnvStore((s) => s.activeEnvId);
  const [pendingEdge, setPendingEdge] = useState<FlowEdge | null>(null);
  const [cycleNodeIds, setCycleNodeIds] = useState<string[]>([]);
  const [cycleEdgeIds, setCycleEdgeIds] = useState<string[]>([]);
  // The full text of the last failed save. The panel shows it for flagged nodes.
  const [saveErrorMessage, setSaveErrorMessage] = useState<string | null>(null);
  // Kept across nodes, so after a run the user can click through Last run.
  const [panelTab, setPanelTab] = useState<PanelTab>('settings');
  // UI state only. The panel opens on request and stays while that node is the sole selection.
  const [selectedNodeIds, setSelectedNodeIds] = useState<ReadonlySet<string>>(() => new Set());
  // The node whose properties panel is open, or null when it is closed.
  const [panelNodeId, setPanelNodeId] = useState<string | null>(null);
  // Set when a palette add opens the panel, so the Label field takes focus.
  const [labelFocusNodeId, setLabelFocusNodeId] = useState<string | null>(null);
  // Set when a node's menu button opens the panel, so the panel takes focus.
  const [panelFocusRequest, setPanelFocusRequest] = useState<{ nodeId: string } | null>(null);
  const canvasAreaRef = useRef<HTMLDivElement>(null);
  // FlowPane is reused across flow tabs, so drop the selection when the tab changes.
  // biome-ignore lint/correctness/useExhaustiveDependencies: tab.id is the trigger.
  useEffect(() => {
    setSelectedNodeIds(new Set());
    setPanelNodeId(null);
    setLabelFocusNodeId(null);
    setPanelFocusRequest(null);
    setSaveErrorMessage(null);
  }, [tab.id]);
  // Any selection other than exactly the panel's node closes the panel.
  const handleSelectedNodeIdsChange = useCallback((ids: ReadonlySet<string>) => {
    setSelectedNodeIds(ids);
    setPanelNodeId((current) =>
      current !== null && ids.size === 1 && ids.has(current) ? current : null,
    );
  }, []);
  // Opens the panel for one node and asks it to take focus.
  const handleOpenProperties = useCallback((nodeId: string) => {
    setPanelNodeId(nodeId);
    setPanelFocusRequest({ nodeId });
  }, []);
  // A removed Auth node's in-memory token goes with it.
  useClearRemovedAuthTokens(tab.collectionName, tab.flowName, tab.nodes);

  // Client-side checks plus whatever the last rejected save named. The popover
  // count and the node badges read this one list.
  const issues = useMemo(
    () =>
      computeFlowIssues(tab.nodes, tab.edges, {
        save:
          cycleNodeIds.length > 0 || cycleEdgeIds.length > 0
            ? { nodeIds: cycleNodeIds, edgeIds: cycleEdgeIds, message: saveErrorMessage }
            : undefined,
      }),
    [tab.nodes, tab.edges, cycleNodeIds, cycleEdgeIds, saveErrorMessage],
  );
  // A deleted node closes its panel.
  useEffect(() => {
    setPanelNodeId((current) =>
      current !== null && tab.nodes.some((n) => n.id === current) ? current : null,
    );
  }, [tab.nodes]);
  // Any other selection ends the focus request, so a later open does not steal focus.
  useEffect(() => {
    setLabelFocusNodeId((current) => (current === panelNodeId ? current : null));
  }, [panelNodeId]);
  useEffect(() => {
    setPanelFocusRequest((current) => (current?.nodeId === panelNodeId ? current : null));
  }, [panelNodeId]);
  // Set by the popover's onCommit, so closing the popover can tell a commit
  // from a cancel.
  const committedEdgeIdRef = useRef<string | null>(null);

  // Inline node edits read the latest graph from the store, not from this
  // render. Two edits before the next render then both land. Stable
  // callbacks also keep FlowCanvas's node actions from changing each render.
  const tabId = tab.id;
  const latestFlowTab = useCallback((): FlowTab | null => {
    const found = findTabInTree(usePaneStore.getState().root, tabId);
    return found && isFlowTab(found.tab) ? found.tab : null;
  }, [tabId]);

  const handleNodeKindChange = useCallback(
    (nodeId: string, kind: FlowNodeKind) => {
      const latest = latestFlowTab();
      if (latest) {
        updateFlowNodes(tabId, replaceNodeKind(latest.nodes, nodeId, kind), {
          coalesceKey: `kind:${nodeId}`,
        });
      }
    },
    [latestFlowTab, tabId, updateFlowNodes],
  );

  const handleRemoveSwitchCase = useCallback(
    (nodeId: string, caseId: string) => {
      const latest = latestFlowTab();
      const next = latest && removeSwitchCase(latest.nodes, latest.edges, nodeId, caseId);
      if (next) updateFlowGraph(tabId, next.nodes, next.edges);
    },
    [latestFlowTab, tabId, updateFlowGraph],
  );

  const focusCanvas = useCallback(() => {
    canvasAreaRef.current?.querySelector<HTMLElement>('[data-testid="flow-canvas"]')?.focus();
  }, []);

  // Removes the node and its wires in one update. It never touches saved
  // requests, and nothing is written until the user saves the flow.
  const handleDeleteNode = useCallback(
    (nodeId: string) => {
      const latest = latestFlowTab();
      if (!latest) return;
      updateFlowGraph(
        tabId,
        latest.nodes.filter((n) => n.id !== nodeId),
        latest.edges.filter((e) => e.sourceNodeId !== nodeId && e.targetNodeId !== nodeId),
      );
      setSelectedNodeIds(new Set());
      setPanelNodeId(null);
      // The panel and its focused button unmount, so keep focus on the canvas.
      focusCanvas();
    },
    [focusCanvas, latestFlowTab, tabId, updateFlowGraph],
  );

  // Undo and redo can leave a failed-save highlight pointing at a changed graph, so clear it.
  const handleUndo = useCallback(() => {
    undoFlow(tabId);
    setCycleNodeIds([]);
    setCycleEdgeIds([]);
    setSaveErrorMessage(null);
  }, [undoFlow, tabId]);
  const handleRedo = useCallback(() => {
    redoFlow(tabId);
    setCycleNodeIds([]);
    setCycleEdgeIds([]);
    setSaveErrorMessage(null);
  }, [redoFlow, tabId]);
  const handleGestureStart = useCallback(() => beginFlowGesture(tabId), [beginFlowGesture, tabId]);
  const handleGestureEnd = useCallback(() => endFlowGesture(tabId), [endFlowGesture, tabId]);

  // Adds a clip to the flow as new nodes in one store write, which is one undo step.
  const pasteClip = useCallback(
    (clip: FlowClip, step: number) => {
      const latest = latestFlowTab();
      if (!latest) return;
      const blocked = canPasteInto(clip, latest.collectionName);
      if (blocked) {
        toast.error(blocked);
        return;
      }
      const result = instantiatePaste(clip, latest.nodes, step);
      updateFlowGraph(
        tabId,
        [...latest.nodes, ...result.nodes],
        [...latest.edges, ...result.edges],
      );
      // The pasted nodes become the selection. The panel stays closed.
      handleSelectedNodeIdsChange(new Set(result.nodes.map((n) => n.id)));
      for (const notice of result.notices) toast.info(notice);
    },
    [latestFlowTab, tabId, updateFlowGraph, handleSelectedNodeIdsChange],
  );

  const handleCopy = useCallback(() => {
    const latest = latestFlowTab();
    if (!latest) return;
    const clip = copySelection(latest.nodes, latest.edges, selectedNodeIds, latest.collectionName);
    if (!clip) return;
    setFlowClipboard(clip);
    toast.info(clip.nodes.length === 1 ? 'Copied 1 node.' : `Copied ${clip.nodes.length} nodes.`);
  }, [latestFlowTab, selectedNodeIds]);

  const handlePaste = useCallback(() => {
    const clip = getFlowClipboard();
    if (clip) pasteClip(clip, nextPasteStep());
  }, [pasteClip]);

  // Duplicating copies and pastes in one go and leaves the clipboard alone.
  const duplicateNodes = useCallback(
    (ids: ReadonlySet<string>) => {
      const latest = latestFlowTab();
      if (!latest) return;
      const clip = copySelection(latest.nodes, latest.edges, ids, latest.collectionName);
      if (clip) pasteClip(clip, 1);
    },
    [latestFlowTab, pasteClip],
  );
  const handleDuplicate = useCallback(
    () => duplicateNodes(selectedNodeIds),
    [duplicateNodes, selectedNodeIds],
  );
  const handleDuplicateNode = useCallback(
    (nodeId: string) => duplicateNodes(new Set([nodeId])),
    [duplicateNodes],
  );

  // Undo and redo can remove a selected node, so drop ids that no longer exist.
  useEffect(() => {
    const live = new Set(tab.nodes.map((n) => n.id));
    setSelectedNodeIds((prev) => pruneSelection(prev, live));
  }, [tab.nodes]);

  // Adds the failed node's label while the node still exists, so the strip
  // keeps naming it after a rename or delete. Then snapshots the run for the
  // history selector.
  const handleRunResult = useCallback(
    (result: FlowRunResult) => {
      const failed = result.failedNodeId
        ? latestFlowTab()?.nodes.find((n) => n.id === result.failedNodeId)
        : undefined;
      const failedLabel = failed ? failed.kind.label || failed.id : undefined;
      setFlowRunResult(tabId, { ...result, ...(failedLabel ? { failedLabel } : {}) });
      // The store has just applied the result, so the tab now holds this run's maps.
      const record = buildRunRecord(latestFlowTab(), result.runId, Date.now());
      if (record) recordFlowRun(tabId, record);
    },
    [latestFlowTab, recordFlowRun, setFlowRunResult, tabId],
  );

  // Selects the node an issue names and opens its Settings tab.
  const handleSelectIssueNode = useCallback(
    (nodeId: string) => {
      setPanelTab('settings');
      handleSelectedNodeIdsChange(new Set([nodeId]));
      handleOpenProperties(nodeId);
    },
    [handleOpenProperties, handleSelectedNodeIdsChange],
  );

  // Selects the node and opens its panel on the Last run tab.
  const handleOpenNodeOnLastRun = useCallback(
    (nodeId: string) => {
      setPanelTab('last-run');
      handleSelectedNodeIdsChange(new Set([nodeId]));
      handleOpenProperties(nodeId);
    },
    [handleOpenProperties, handleSelectedNodeIdsChange],
  );

  if (tab.flowName === null) {
    return <FlowPicker tab={tab} groupId={groupId} />;
  }

  // tab.flowName is narrowed non-null by the picker-state return above.
  // tab.collectionName is a separate optional field on FlowTab; in practice
  // it is always set alongside a resolved flowName (the picker only calls
  // openFlowTab with a chosen collection), but guard it explicitly rather
  // than asserting, since a null value here would misdirect run/save calls.
  const flowName = tab.flowName;
  const collectionName = tab.collectionName;
  if (!collectionName) {
    console.error('[FlowPane] flow tab is missing a collection name');
    return null;
  }

  // A new node becomes the only selection, and its properties panel opens.
  const handleAddNode = (node: FlowNode) => {
    updateFlowNodes(tab.id, [...tab.nodes, node]);
    setSelectedNodeIds(new Set([node.id]));
    setPanelNodeId(node.id);
    setLabelFocusNodeId(node.id);
  };

  // Opens the script dialog of a wire. Run when wires carry no value.
  const openWireEditor = (edgeId: string) => {
    const edge = tab.edges.find((e) => e.id === edgeId);
    if (edge && shouldPromptForExpression(edge)) setPendingEdge(edge);
  };

  // Returns whether the save succeeded, so Run can stop on a failed save.
  const handleSave = async (quiet = false): Promise<boolean> => {
    try {
      const payload = flowPayloadFromTab(tab);
      if (!payload) return false;
      // The graph being written, so an edit made during the save stays unsaved.
      const written = snapOf(tab);
      await saveFlow(payload.collection, payload.flow);
      setCycleNodeIds([]);
      setCycleEdgeIds([]);
      setSaveErrorMessage(null);
      markClean(tab.id, written);
      if (!quiet) toast.success('Flow saved.');
      return true;
    } catch (err) {
      // Any validation error names the offending node(s) and edge(s) at the
      // end of the message. Flag them on the canvas as well as toasting.
      const message = String(err);
      const parsed = parseGraphErrorMessage(message);
      if (parsed) {
        setCycleNodeIds(parsed.nodeIds);
        setCycleEdgeIds(parsed.edgeIds);
        setSaveErrorMessage(message);
      }
      toast.error(`Could not save flow: ${message}`);
      return false;
    }
  };

  // `run_flow` runs the saved file, not the canvas. Save unsaved edits
  // first, so Run executes what the user sees.
  const handleBeforeRun = async () => {
    if (!tab.isDirty) return true;
    const saved = await handleSave(true);
    if (saved) toast.info('Flow saved before run.');
    return saved;
  };

  // A bare `headers` target is not a valid target_field (the backend
  // rejects it). If a headers-target edge's popover is dismissed — or
  // preempted by a new connection — without a commit, that edge must be
  // dropped instead of left to fail the run.
  const isUncommittedHeadersEdge = (edge: FlowEdge) =>
    committedEdgeIdRef.current !== edge.id && edge.targetField === 'headers';

  const handleConnect = (connection: Connection) => {
    const sourceNode = tab.nodes.find((n) => n.id === connection.source);
    if (!sourceNode) return;
    const edge = buildEdgeFromConnection(connection, sourceNode);
    if (!edge) return;
    // A connection made while a previous popover is still open (before it
    // was committed or dismissed) preempts it here — React never runs the
    // popover's onOpenChange in that case, so drop its uncommitted pending
    // edge in this same update rather than leaving it dangling.
    const base =
      pendingEdge && isUncommittedHeadersEdge(pendingEdge)
        ? tab.edges.filter((e) => e.id !== pendingEdge.id)
        : tab.edges;
    // The connect and the dialog's commit or dismissal share one undo step.
    updateFlowEdges(tab.id, [...base, edge], WIRE_STEP(edge.id));
    // Input and trigger wires carry no value, so there is nothing to edit.
    // Clearing the pending edge also closes a preempted popover.
    setPendingEdge(shouldPromptForExpression(edge) ? edge : null);
  };

  const pendingTargetNode = pendingEdge
    ? tab.nodes.find((n) => n.id === pendingEdge.targetNodeId)
    : undefined;

  const panelNode = panelNodeId ? tab.nodes.find((n) => n.id === panelNodeId) : undefined;
  // A past run chosen in the selector replaces the live results on screen. An id
  // that is no longer in the history counts as live. The toolbar and the run
  // announcer keep reading the live maps.
  const viewedRecord = tab.viewedRunId
    ? ((tab.runHistory ?? []).find((r) => r.runId === tab.viewedRunId) ?? null)
    : null;
  const shownStatus = viewedRecord ? viewedRecord.nodeStatus : tab.nodeStatus;
  const shownDetail = viewedRecord ? viewedRecord.nodeDetail : tab.nodeDetail;
  const shownRun = viewedRecord ? viewedRecord.result : tab.lastRun;

  return (
    <ResizablePanelGroup className='h-full'>
      <ResizablePanel id='flow-canvas-panel' minSize='40%'>
        <div ref={canvasAreaRef} className='relative h-full'>
          <FlowSaveShortcut
            tabId={tab.id}
            onSave={() => handleSave()}
            isDirty={() => latestFlowTab()?.isDirty ?? false}
          />
          <div className='absolute top-2 right-2 z-10 flex max-w-[calc(100%-1rem)] flex-wrap items-center justify-end gap-2'>
            {tab.nodes.some((n) => n.kind.kind === 'WaitForCallback') && (
              <CallbackHostSetting
                value={tab.callbackHost}
                onChange={(host) =>
                  setFlowCallbackHost(tab.id, host, { coalesceKey: 'callback-host' })
                }
              />
            )}
            <FlowHistoryButtons
              canUndo={(tab.history?.past.length ?? 0) > 0}
              canRedo={(tab.history?.future.length ?? 0) > 0}
              onUndo={() => {
                handleUndo();
                focusCanvas();
              }}
              onRedo={() => {
                handleRedo();
                focusCanvas();
              }}
            />
            <RunHistorySelect
              history={tab.runHistory ?? []}
              viewedRunId={viewedRecord ? viewedRecord.runId : null}
              disabled={tab.runState === 'running'}
              onChange={(runId) => setViewedFlowRun(tab.id, runId)}
            />
            <FlowIssuesButton
              issues={issues}
              nodes={tab.nodes}
              onSelectNode={handleSelectIssueNode}
            />
            <FlowToolbar
              collection={collectionName}
              flowName={flowName}
              tabId={tab.id}
              environmentName={activeEnvironmentName}
              onPatchStatus={(nodeId, status, detail) =>
                patchFlowNodeStatus(tab.id, nodeId, status as FlowNodeStatus, detail)
              }
              onPatchProgress={(nodeId, message, live) =>
                patchFlowNodeProgress(tab.id, nodeId, message, live)
              }
              onCallbackUrls={(urls) => setFlowCallbackUrls(tab.id, urls)}
              onRunStateChange={(state, runId) => setFlowRunState(tab.id, state, runId)}
              onRunRequested={(runId) => setFlowPendingRun(tab.id, runId)}
              tabRunState={tab.runState}
              tabRunId={tab.runId}
              tabPendingRunId={tab.pendingRunId}
              onBeforeRun={handleBeforeRun}
              onPrepareAuth={() =>
                collectFlowAuthTokens({
                  collection: collectionName,
                  flowName,
                  // Read at click time, after onBeforeRun saved unsaved edits.
                  nodes: latestFlowTab()?.nodes ?? tab.nodes,
                  environmentName: activeEnvironmentName ?? undefined,
                  globalEnvName: getActiveGlobalEnvName(),
                })
              }
              onStepLogs={(nodeId, logs) => {
                const node = latestFlowTab()?.nodes.find((n) => n.id === nodeId);
                const label = node?.kind.label || nodeId;
                useConsoleStore.getState().addScriptEntries(
                  logs.map((l) => ({
                    level: l.level,
                    message: l.message,
                    requestName: `${flowName} › ${label}`,
                  })),
                );
              }}
              onRunResult={handleRunResult}
              onStepDebug={(nodeId, debug) => {
                const node = latestFlowTab()?.nodes.find((n) => n.id === nodeId);
                const label = node?.kind.label || nodeId;
                const response = debug.response;
                useConsoleStore.getState().addHttpEntry({
                  requestName: `${flowName} › ${label}`,
                  method: debug.method,
                  url: debug.url,
                  status: response?.status ?? 0,
                  statusText: response?.statusText ?? 'Error',
                  durationMs: response?.durationMs ?? 0,
                  sizeBytes: response?.sizeBytes ?? 0,
                  requestHeaders: debug.headers,
                  requestBody: debug.body ?? '',
                  responseHeaders: response?.headers ?? [],
                  responseBody: response?.body ?? debug.error ?? '',
                });
              }}
            />
            <Button size='sm' variant='outline' onClick={() => void handleSave()}>
              Save
            </Button>
            <FlowExportMenu tab={tab} />
          </div>
          {shownRun && (
            <div className='absolute top-12 right-2 z-10 max-w-[60%]'>
              <RunResultStrip
                result={shownRun}
                canSelectFailed={tab.nodes.some((n) => n.id === shownRun.failedNodeId)}
                onSelectFailed={handleOpenNodeOnLastRun}
              />
            </div>
          )}
          {viewedRecord && (
            <div className='absolute top-14 left-3 z-10 max-w-[45%]'>
              <ViewedRunBanner
                record={viewedRecord}
                onBack={() => setViewedFlowRun(tab.id, null)}
              />
            </div>
          )}
          <FlowRunAnnouncer
            key={tab.id}
            nodes={tab.nodes}
            nodeStatus={tab.nodeStatus}
            nodeDetail={tab.nodeDetail}
            runState={tab.runState}
          />
          <NodePalette onAddNode={handleAddNode} nodes={tab.nodes} />
          <FlowCanvas
            nodes={tab.nodes}
            edges={tab.edges}
            nodeStatus={shownStatus}
            nodeDetail={shownDetail}
            issues={issues}
            cycleEdgeIds={cycleEdgeIds}
            onNodesChange={(nodes, options) => updateFlowNodes(tab.id, nodes, options)}
            onEdgesChange={(edges, options) => updateFlowEdges(tab.id, edges, options)}
            onGestureStart={handleGestureStart}
            onGestureEnd={handleGestureEnd}
            onUndo={handleUndo}
            onRedo={handleRedo}
            onCopy={handleCopy}
            onPaste={handlePaste}
            onDuplicate={handleDuplicate}
            onDuplicateNode={handleDuplicateNode}
            onConnect={handleConnect}
            onAddNode={handleAddNode}
            flowCollectionName={tab.collectionName}
            onNodeKindChange={handleNodeKindChange}
            onRemoveSwitchCase={handleRemoveSwitchCase}
            selectedNodeIds={selectedNodeIds}
            onSelectedNodeIdsChange={handleSelectedNodeIdsChange}
            onOpenProperties={handleOpenProperties}
            onEdgeEdit={openWireEditor}
            callbackUrls={tab.callbackUrls}
          />
          {pendingEdge && pendingTargetNode && (
            <WireScriptDialog
              // Keyed by edge id so a second connection made before the first
              // dialog is committed/dismissed remounts this component instead
              // of reusing it — otherwise its internal `expression`/`headerName`
              // state (initialized once via useState) would leak from the
              // previous edge onto the new one.
              key={pendingEdge.id}
              edge={pendingEdge}
              targetNode={pendingTargetNode}
              open={pendingEdge !== null}
              onCloseFocus={focusCanvas}
              onOpenChange={(open) => {
                if (open) return;
                if (isUncommittedHeadersEdge(pendingEdge)) {
                  updateFlowEdges(
                    tab.id,
                    tab.edges.filter((e) => e.id !== pendingEdge.id),
                    WIRE_STEP(pendingEdge.id),
                  );
                }
                setPendingEdge(null);
              }}
              onCommit={(updated) => {
                committedEdgeIdRef.current = updated.id;
                updateFlowEdges(
                  tab.id,
                  tab.edges.map((e) => (e.id === updated.id ? updated : e)),
                  WIRE_STEP(updated.id),
                );
              }}
            />
          )}
        </div>
      </ResizablePanel>
      {panelNode && (
        <>
          {/* The nokey class keeps Backspace on the focused handle away from React Flow. */}
          <ResizableHandle className='nokey' />
          {/* All three sizes are percentage strings. A plain number means
              pixels in this version of react-resizable-panels. */}
          <ResizablePanel id='flow-node-properties' defaultSize='30%' minSize='20%' maxSize='50%'>
            <NodePropertiesPanel
              node={panelNode}
              edges={tab.edges}
              nodes={tab.nodes}
              collection={collectionName}
              flowName={flowName}
              status={shownStatus[panelNode.id] ?? 'idle'}
              detail={shownDetail?.[panelNode.id]}
              nodeStatus={shownStatus}
              nodeDetail={shownDetail}
              saveError={
                saveErrorMessage && cycleNodeIds.includes(panelNode.id)
                  ? saveErrorMessage
                  : undefined
              }
              activeTab={panelTab}
              onTabChange={setPanelTab}
              onEditWire={openWireEditor}
              onSelectNode={(nodeId) => {
                setSelectedNodeIds(new Set([nodeId]));
                setPanelNodeId(nodeId);
              }}
              onChange={(kind) => handleNodeKindChange(panelNode.id, kind)}
              onClose={() => {
                setSelectedNodeIds(new Set());
                setPanelNodeId(null);
              }}
              onDelete={() => handleDeleteNode(panelNode.id)}
              focusRequest={panelFocusRequest}
              autoFocusLabel={panelNode.id === labelFocusNodeId}
              callbackUrl={tab.callbackUrls?.[panelNode.id]}
            />
          </ResizablePanel>
        </>
      )}
    </ResizablePanelGroup>
  );
}
