// src/components/flow/properties/WiresTab.tsx
import { ArrowLeft, ArrowRight, ChevronRight, Pencil } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import type { FlowEdge, FlowNode, FlowNodeStatus } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import type { FlowNodeDetail } from '@/types/pane-types';
import { usePanelRefocus } from './panelFocus';
import { incomingRows, outgoingGroups, type WireResolved, type WireRow } from './wireRows';

interface WiresTabProps {
  node: FlowNode;
  nodes: FlowNode[];
  edges: FlowEdge[];
  nodeStatus?: Record<string, FlowNodeStatus>;
  nodeDetail?: Record<string, FlowNodeDetail>;
  onEditWire: (edgeId: string) => void;
  onSelectNode: (nodeId: string) => void;
}

function NodeLink({ row, onSelectNode }: { row: WireRow; onSelectNode: (id: string) => void }) {
  const refocusPanel = usePanelRefocus();
  if (row.otherLabel === null) {
    return <span className='italic text-muted-foreground'>(missing node)</span>;
  }
  return (
    <Button
      type='button'
      variant='link'
      className='h-auto min-w-0 max-w-full truncate p-0 text-xs'
      title={row.otherLabel}
      aria-label={`Select node ${row.otherLabel}`}
      // Selecting another node re-renders the tab and drops this button, so
      // focus goes back to the panel instead of the body.
      onClick={() => {
        onSelectNode(row.otherNodeId);
        refocusPanel();
      }}
    >
      {row.otherLabel}
    </Button>
  );
}

// The value the last run put on a wire. Collapsed, because one can be 16 KB.
function ResolvedValue({ resolved }: { resolved: WireResolved }) {
  const [open, setOpen] = useState(false);
  if (resolved.credential) {
    return (
      <p data-testid='wire-credential' className='italic text-muted-foreground'>
        Credential (hidden)
      </p>
    );
  }
  if (resolved.error) {
    return (
      <p
        data-testid='wire-error'
        className='select-text whitespace-pre-wrap break-words text-red-600'
      >
        {resolved.error}
      </p>
    );
  }
  if (resolved.value === undefined) return null;
  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger asChild>
        <Button type='button' variant='ghost' size='sm' className='h-5 px-1 text-[11px]'>
          <ChevronRight
            className={
              open ? 'h-3 w-3 rotate-90 transition-transform' : 'h-3 w-3 transition-transform'
            }
            aria-hidden='true'
          />
          Last value
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <pre
          data-testid='wire-value'
          className='mt-1 max-h-40 select-text overflow-auto whitespace-pre-wrap rounded-md border p-1.5 font-mono text-[11px] [overflow-wrap:anywhere]'
        >
          {resolved.value === '' ? <span className='italic'>(empty)</span> : resolved.value}
        </pre>
        {resolved.truncated && <p className='text-muted-foreground'>Cut at 16 KB.</p>}
      </CollapsibleContent>
    </Collapsible>
  );
}

function Row({
  row,
  direction,
  onEditWire,
  onSelectNode,
}: {
  row: WireRow;
  direction: 'in' | 'out';
  onEditWire: (id: string) => void;
  onSelectNode: (id: string) => void;
}) {
  return (
    <div
      data-testid='wire-row'
      data-failed={row.failed ? 'true' : undefined}
      className={cn(
        'flex items-start gap-1.5 rounded-md border px-2 py-1.5',
        row.notTaken && 'opacity-50',
        row.failed && 'border-red-500/60 bg-red-500/5',
      )}
    >
      <div className='min-w-0 flex-1 space-y-0.5'>
        <div className='flex flex-wrap items-center gap-1'>
          {direction === 'in' ? (
            <>
              <span className='font-medium'>{row.field}</span>
              <ArrowLeft className='h-3 w-3 text-muted-foreground' aria-hidden='true' />
              <NodeLink row={row} onSelectNode={onSelectNode} />
              {row.exit && <span className='text-muted-foreground'>· {row.exit}</span>}
            </>
          ) : (
            <>
              <ArrowRight className='h-3 w-3 text-muted-foreground' aria-hidden='true' />
              <NodeLink row={row} onSelectNode={onSelectNode} />
              <span className='text-muted-foreground'>· {row.field}</span>
            </>
          )}
          {row.notTaken && <span className='italic text-muted-foreground'>not taken</span>}
        </div>
        <p className='truncate font-mono text-[11px] text-muted-foreground'>
          {row.preview ?? '(no script)'}
        </p>
        {row.resolved && <ResolvedValue resolved={row.resolved} />}
      </div>
      {row.editable && (
        <Button
          type='button'
          variant='ghost'
          size='icon'
          className='h-5 w-5 shrink-0'
          aria-label={`Edit wire into ${row.field}`}
          title='Edit script'
          onClick={() => onEditWire(row.edgeId)}
        >
          <Pencil className='h-3 w-3' aria-hidden='true' />
        </Button>
      )}
    </div>
  );
}

export function WiresTab({
  node,
  nodes,
  edges,
  nodeStatus,
  nodeDetail,
  onEditWire,
  onSelectNode,
}: WiresTabProps) {
  const incoming = incomingRows(node, nodes, edges, nodeStatus, nodeDetail);
  const outgoing = outgoingGroups(node, nodes, edges, nodeStatus, nodeDetail);

  if (incoming.length === 0 && outgoing.length === 0) {
    return (
      <p className='text-xs text-muted-foreground'>
        No wires. Drag from a dot on the canvas to connect nodes.
      </p>
    );
  }

  return (
    <div className='space-y-3 text-xs'>
      {incoming.length > 0 && (
        <section data-testid='wires-incoming' className='space-y-1.5'>
          <h4 className='font-medium'>Incoming</h4>
          {incoming.map((row) => (
            <Row
              key={row.edgeId}
              row={row}
              direction='in'
              onEditWire={onEditWire}
              onSelectNode={onSelectNode}
            />
          ))}
        </section>
      )}
      {outgoing.length > 0 && (
        <section data-testid='wires-outgoing' className='space-y-2'>
          <h4 className='font-medium'>Outgoing</h4>
          {outgoing.map((group) => (
            <div
              key={group.handle ?? 'result'}
              data-testid='wires-exit-group'
              data-exit={group.exit ?? 'result'}
              className='space-y-1.5'
            >
              {group.exit && <p className='text-muted-foreground'>{group.exit}</p>}
              {group.rows.map((row) => (
                <Row
                  key={row.edgeId}
                  row={row}
                  direction='out'
                  onEditWire={onEditWire}
                  onSelectNode={onSelectNode}
                />
              ))}
            </div>
          ))}
        </section>
      )}
    </div>
  );
}
