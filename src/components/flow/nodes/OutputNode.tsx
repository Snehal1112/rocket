import { Handle, type NodeProps, Position } from '@xyflow/react';
import type { FlowNodeKind, FlowNodeStatus } from '@/lib/tauri-api';

export interface OutputNodeData {
  kind: Extract<FlowNodeKind, { kind: 'Output' }>;
  status: FlowNodeStatus;
  result?: string;
}

export function OutputNode({ data, isConnectable }: NodeProps & { data: OutputNodeData }) {
  return (
    <div
      data-testid='output-node-card'
      className='w-48 rounded-md border bg-card text-card-foreground text-xs shadow-sm'
    >
      <Handle
        type='target'
        id='value'
        position={Position.Left}
        isConnectable={isConnectable}
        className='!h-2 !w-2'
      />
      <div className='border-b px-2 py-1.5 font-medium'>{data.kind.label}</div>
      <div className='truncate px-2 py-1.5 text-muted-foreground'>{data.result ?? '—'}</div>
    </div>
  );
}
