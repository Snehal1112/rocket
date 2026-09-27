import { Handle, type NodeProps, Position } from '@xyflow/react';
import type { FlowNodeKind, FlowNodeStatus } from '@/lib/tauri-api';

export interface InputNodeData {
  kind: Extract<FlowNodeKind, { kind: 'Input' }>;
  status: FlowNodeStatus;
}

export function InputNode({ data, isConnectable }: NodeProps & { data: InputNodeData }) {
  const value = data.kind.value;
  const display = value === undefined || value === null ? '—' : String(value);
  return (
    <div
      data-testid='input-node-card'
      className='w-48 rounded-md border bg-card text-card-foreground text-xs shadow-sm'
    >
      <div className='border-b px-2 py-1.5 font-medium'>{data.kind.label}</div>
      <div className='truncate px-2 py-1.5 text-muted-foreground'>{display}</div>
      <Handle
        type='source'
        id='result'
        position={Position.Right}
        isConnectable={isConnectable}
        className='!h-2 !w-2'
      />
    </div>
  );
}
