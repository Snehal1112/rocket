import { Handle, type NodeProps, Position } from '@xyflow/react';
import { RESULT_HANDLE } from '@/lib/flow-handles';
import type { FlowIssue } from '@/lib/flow-issues';
import type { FlowNodeKind, FlowNodeStatus, FlowSkipReason } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { DurationChip } from './DurationChip';
import { NodeIssueBadge } from './NodeIssueBadge';
import { NodeMenuButton } from './NodeMenuButton';
import { NodeStatusCaption } from './NodeStatusCaption';
import { NodeStatusIcon } from './NodeStatusIcon';
import { issueRingClassName, nodeStatusClassName } from './nodeStatus';

export interface InputNodeData {
  kind: Extract<FlowNodeKind, { kind: 'Input' }>;
  status: FlowNodeStatus;
  skipReason?: FlowSkipReason;
  error?: string;
  /** Progress text while running, such as "attempt 3/30". */
  progress?: string;
  /** How long the last run of this node took. */
  durationMs?: number;
  /** Problems found in this node, drawn as a ring and a badge. */
  issues?: FlowIssue[];
}

export function InputNode({ id, data, isConnectable }: NodeProps & { data: InputNodeData }) {
  const value = data.kind.value;
  const display = value === undefined || value === null ? '—' : String(value);
  return (
    <div
      data-testid='input-node-card'
      data-status={data.status}
      className={cn(
        'w-48 rounded-md border bg-card text-card-foreground text-xs shadow-sm',
        nodeStatusClassName(data.status, data.skipReason),
        issueRingClassName(data.issues),
      )}
    >
      <div className='flex items-center gap-1.5 border-b px-2 py-1.5 font-medium'>
        <span className='truncate'>{data.kind.label}</span>
        <NodeIssueBadge issues={data.issues} />
        <NodeStatusIcon status={data.status} />
        <NodeMenuButton nodeId={id} label={data.kind.label} />
      </div>
      <NodeStatusCaption
        status={data.status}
        skipReason={data.skipReason}
        error={data.error}
        progress={data.progress}
      />
      <DurationChip durationMs={data.durationMs} />
      <div className='truncate px-2 py-1.5 text-muted-foreground'>{display}</div>
      <Handle
        type='source'
        id={RESULT_HANDLE}
        position={Position.Right}
        isConnectable={isConnectable}
        className='!h-2 !w-2'
      />
    </div>
  );
}
