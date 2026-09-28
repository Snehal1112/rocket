import { Handle, type NodeProps, Position } from '@xyflow/react';
import type { FlowNodeKind, FlowNodeStatus } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';

export interface OutputNodeData {
  kind: Extract<FlowNodeKind, { kind: 'Output' }>;
  status: FlowNodeStatus;
  /** Set when a save was rejected because this node is part of a cycle. */
  hasCycleError?: boolean;
  value?: string;
}

export function OutputNode({ data, isConnectable }: NodeProps & { data: OutputNodeData }) {
  return (
    <div
      data-testid='output-node-card'
      className={cn(
        'w-48 rounded-md border bg-card text-card-foreground text-xs shadow-sm',
        data.hasCycleError && 'ring-2 ring-red-500',
      )}
    >
      {/* Data-less "Run when" input. It sits at the top so it does not overlap `value`. */}
      <Handle
        type='target'
        id='trigger'
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
      <div className='border-b px-2 py-1.5 font-medium'>{data.kind.label}</div>
      <div className='truncate px-2 py-1.5 text-muted-foreground'>{data.value ?? '—'}</div>
    </div>
  );
}
