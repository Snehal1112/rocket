import { Handle, type NodeProps, Position } from '@xyflow/react';
import { TRIGGER_HANDLE } from '@/lib/flow-handles';
import type { FlowNodeKind, FlowNodeStatus, FlowSkipReason } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { NodeMenuButton } from './NodeMenuButton';
import { NodeStatusCaption } from './NodeStatusCaption';
import { nodeStatusClassName } from './nodeStatus';

export interface OutputNodeData {
  kind: Extract<FlowNodeKind, { kind: 'Output' }>;
  status: FlowNodeStatus;
  skipReason?: FlowSkipReason;
  error?: string;
  /** Set when this node is named in a save validation error, such as a cycle. */
  hasCycleError?: boolean;
  value?: string;
}

export function OutputNode({ id, data, isConnectable }: NodeProps & { data: OutputNodeData }) {
  return (
    <div
      data-testid='output-node-card'
      data-status={data.status}
      className={cn(
        'w-48 rounded-md border bg-card text-card-foreground text-xs shadow-sm',
        nodeStatusClassName(data.status, data.skipReason),
        data.hasCycleError && 'ring-2 ring-red-500',
      )}
    >
      {/* Data-less "Run when" input. It sits at the top so it does not overlap `value`. */}
      <Handle
        type='target'
        id={TRIGGER_HANDLE}
        title='Run when'
        position={Position.Left}
        isConnectable={isConnectable}
        style={{ top: 10 }}
        className='!h-2 !w-2'
      />
      <Handle
        type='target'
        id='value'
        position={Position.Left}
        isConnectable={isConnectable}
        className='!h-2 !w-2'
      />
      <div className='flex items-center gap-1.5 border-b px-2 py-1.5 font-medium'>
        <span className='truncate'>{data.kind.label}</span>
        <NodeMenuButton nodeId={id} label={data.kind.label} />
      </div>
      <NodeStatusCaption status={data.status} skipReason={data.skipReason} error={data.error} />
      <div className='truncate px-2 py-1.5 text-muted-foreground'>{data.value ?? '—'}</div>
    </div>
  );
}
