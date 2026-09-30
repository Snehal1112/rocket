import { Trash2, X } from 'lucide-react';
import { useCallback, useEffect, useRef } from 'react';
import { Button } from '@/components/ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import type { FlowEdge, FlowNode, FlowNodeKind, FlowNodeStatus } from '@/lib/tauri-api';
import type { FlowNodeDetail } from '@/types/pane-types';
import { InputNodeEditor } from './InputNodeEditor';
import { LabelOnlyEditor } from './LabelOnlyEditor';
import { LastRunTab } from './LastRunTab';
import { IfDetails, OutputDetails, SwitchDetails } from './NodeDetails';
import { PanelFocusProvider } from './panelFocus';
import { RequestNodeEditor } from './RequestNodeEditor';
import { WaitForCallbackEditor } from './WaitForCallbackEditor';
import { WiresTab } from './WiresTab';

/** The panel's tabs. The selected one is kept by FlowPane across nodes. */
export type PanelTab = 'settings' | 'last-run' | 'wires';

// Picks the editor for the node's kind. Each editor reports a whole new kind,
// and the caller applies it with one store update.
function editorFor(
  node: FlowNode,
  edges: FlowEdge[],
  nodes: FlowNode[],
  collection: string,
  onChange: (kind: FlowNodeKind) => void,
) {
  const kind = node.kind;
  switch (kind.kind) {
    case 'Request':
      return (
        <RequestNodeEditor
          // Keyed by node, so a pending confirmation never carries over to another node.
          key={node.id}
          nodeId={node.id}
          kind={kind}
          edges={edges}
          collection={collection}
          onChange={onChange}
        />
      );
    case 'Input':
      return <InputNodeEditor kind={kind} onChange={onChange} />;
    case 'If':
      return (
        <div className='space-y-3'>
          <LabelOnlyEditor kind={kind} onChange={onChange} />
          <IfDetails condition={kind.condition} />
        </div>
      );
    case 'Switch':
      return (
        <div className='space-y-3'>
          <LabelOnlyEditor kind={kind} onChange={onChange} />
          <SwitchDetails value={kind.value} cases={kind.cases} />
        </div>
      );
    case 'Output':
      return (
        <div className='space-y-3'>
          <LabelOnlyEditor kind={kind} onChange={onChange} />
          <OutputDetails nodeId={node.id} edges={edges} nodes={nodes} />
        </div>
      );
    case 'WaitForCallback':
      return <WaitForCallbackEditor kind={kind} onChange={onChange} />;
    case 'Transform':
      // Plan 05 replaces this with the script editor.
      return <LabelOnlyEditor kind={kind} onChange={onChange} />;
  }
}

export function NodePropertiesPanel({
  node,
  edges,
  nodes,
  collection,
  status,
  detail,
  nodeStatus,
  nodeDetail,
  saveError,
  activeTab,
  onTabChange,
  onEditWire,
  onSelectNode,
  onChange,
  onClose,
  onDelete,
  autoFocusLabel = false,
  focusRequest = null,
}: {
  node: FlowNode;
  edges: FlowEdge[];
  nodes: FlowNode[];
  collection: string;
  status: FlowNodeStatus;
  detail?: FlowNodeDetail;
  /** Every node's last-run status, so the Wires tab can fade wires not taken. */
  nodeStatus?: Record<string, FlowNodeStatus>;
  /** Every node's last-run detail; the Wires tab reads each routing node's branch. */
  nodeDetail?: Record<string, FlowNodeDetail>;
  // The full message of the last failed save, when it named this node.
  saveError?: string;
  activeTab: PanelTab;
  onTabChange: (tab: PanelTab) => void;
  // Opens the script dialog of a wire, as a double-click on the canvas does.
  onEditWire: (edgeId: string) => void;
  // Selects another node and shows it in this panel.
  onSelectNode: (nodeId: string) => void;
  onChange: (kind: FlowNodeKind) => void;
  onClose: () => void;
  onDelete: () => void;
  autoFocusLabel?: boolean;
  // A new object asks the panel to take focus, as when a node's menu button opens it.
  focusRequest?: { nodeId: string } | null;
}) {
  const asideRef = useRef<HTMLElement>(null);
  const refocusPanel = useCallback(() => asideRef.current?.focus(), []);
  // A menu returns focus to its trigger as it closes, so wait a frame and a tick
  // before moving focus to the Label field.
  // biome-ignore lint/correctness/useExhaustiveDependencies: node.id is the trigger.
  useEffect(() => {
    if (!autoFocusLabel) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const frame = requestAnimationFrame(() => {
      timer = setTimeout(() => {
        const field = asideRef.current?.querySelector<HTMLInputElement>('#flow-node-label');
        field?.focus();
        field?.select();
      }, 0);
    });
    return () => {
      cancelAnimationFrame(frame);
      clearTimeout(timer);
    };
  }, [autoFocusLabel, node.id]);

  // Moves focus off the menu button and into the panel, which is a `nokey` area.
  useEffect(() => {
    if (focusRequest && focusRequest.nodeId === node.id) asideRef.current?.focus();
  }, [focusRequest, node.id]);

  const deleteTitle =
    node.kind.kind === 'Request' && node.kind.source.type === 'Saved'
      ? 'Removes this node from the flow. The saved request is not deleted.'
      : 'Removes this node and its wires from the flow.';

  return (
    <aside
      ref={asideRef}
      tabIndex={-1}
      aria-label='Node properties'
      data-testid='node-properties-panel'
      className='nokey flex h-full flex-col bg-background outline-none'
    >
      <div className='flex items-center justify-between gap-2 border-b px-3 py-2'>
        <span className='truncate text-xs font-medium'>
          {node.kind.kind} · {node.kind.label}
        </span>
        <div className='flex items-center gap-1'>
          <Button
            type='button'
            variant='ghost'
            size='icon'
            className='h-6 w-6'
            aria-label='Delete node'
            title={deleteTitle}
            onClick={onDelete}
          >
            <Trash2 className='h-3.5 w-3.5' aria-hidden='true' />
          </Button>
          <Button
            type='button'
            variant='ghost'
            size='icon'
            className='h-6 w-6'
            aria-label='Close properties'
            onClick={onClose}
          >
            <X className='h-3.5 w-3.5' aria-hidden='true' />
          </Button>
        </div>
      </div>
      {/* Every tab can remove a focused control, so each one gets the refocus helper. */}
      <PanelFocusProvider value={refocusPanel}>
        <Tabs
          value={activeTab}
          onValueChange={(value) => onTabChange(value as PanelTab)}
          className='flex min-h-0 flex-1 flex-col'
        >
          <TabsList className='mx-3 mt-2 self-start'>
            <TabsTrigger value='settings' className='text-xs'>
              Settings
            </TabsTrigger>
            <TabsTrigger value='last-run' className='text-xs'>
              Last run
            </TabsTrigger>
            <TabsTrigger value='wires' className='text-xs'>
              Wires
            </TabsTrigger>
          </TabsList>
          <TabsContent value='settings' className='min-h-0 flex-1 overflow-y-auto p-3'>
            <div key={node.id} className='space-y-3'>
              {saveError && (
                <p
                  data-testid='node-save-error'
                  className='break-words rounded border border-red-500/50 bg-red-500/10 p-2 text-xs text-red-600'
                >
                  {saveError}
                </p>
              )}
              {editorFor(node, edges, nodes, collection, onChange)}
            </div>
          </TabsContent>
          <TabsContent value='last-run' className='min-h-0 flex-1 overflow-y-auto p-3'>
            <LastRunTab node={node} status={status} detail={detail} />
          </TabsContent>
          <TabsContent value='wires' className='min-h-0 flex-1 overflow-y-auto p-3'>
            <WiresTab
              node={node}
              nodes={nodes}
              edges={edges}
              nodeStatus={nodeStatus}
              nodeDetail={nodeDetail}
              onEditWire={onEditWire}
              onSelectNode={onSelectNode}
            />
          </TabsContent>
        </Tabs>
      </PanelFocusProvider>
    </aside>
  );
}
