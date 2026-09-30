// src/components/flow/properties/WiresTab.tsx
import { ArrowLeft, ArrowRight, Pencil } from 'lucide-react';
import { Button } from '@/components/ui/button';
import type { FlowEdge, FlowNode, FlowNodeStatus } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import type { FlowNodeDetail } from '@/types/pane-types';
import { incomingRows, outgoingGroups, type WireRow } from './wireRows';

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
      // The row itself may open the wire, so the link must not bubble up.
      onClick={(e) => {
        e.stopPropagation();
        onSelectNode(row.otherNodeId);
      }}
    >
      {row.otherLabel}
    </Button>
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
      className={cn(
        'flex items-start gap-1.5 rounded-md border px-2 py-1.5',
        row.notTaken && 'opacity-50',
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
