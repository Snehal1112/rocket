import { Handle, type NodeProps, Position } from '@xyflow/react';
import { Code } from 'lucide-react';
import { INPUT_HANDLE, RESULT_HANDLE } from '@/lib/flow-handles';
import type { FlowNodeKind, FlowNodeStatus, FlowSkipReason } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { scriptPreview } from '../properties/wireRows';
import { DurationChip } from './DurationChip';
import { NodeMenuButton } from './NodeMenuButton';
import { NodeStatusCaption } from './NodeStatusCaption';
import { nodeStatusClassName } from './nodeStatus';

export type TransformNodeData = {
  kind: Extract<FlowNodeKind, { kind: 'Transform' }>;
  status: FlowNodeStatus;
  error?: string;
  /** Progress text while running. */
  progress?: string;
  /** How long the last run of this node took. */
  durationMs?: number;
  skipReason?: FlowSkipReason;
  /** Set when a save was rejected because of this node. */
  hasCycleError?: boolean;
};

export function TransformNode({
  id,
  data,
  isConnectable,
}: NodeProps & { data: TransformNodeData }) {
  const { kind, status } = data;
  // The script is edited in the properties panel, so the node shows one line.
  const preview = scriptPreview(kind.script);
  // The hover title shows the whole first line, which the card may cut off.
  const firstLine = kind.script
    .split('\n')
    .map((line) => line.trim())
    .find((line) => line !== '');

  return (
    <div
      data-testid='transform-node-card'
      data-status={status}
      className={cn(
        'w-64 rounded-md border bg-card text-card-foreground text-xs shadow-sm',
        nodeStatusClassName(status, data.skipReason),
        data.hasCycleError && 'ring-2 ring-red-500',
      )}
    >
      <Handle
        type='target'
        id={INPUT_HANDLE}
        position={Position.Left}
        isConnectable={isConnectable}
        className='!h-2 !w-2'
      />
      <div className='flex items-center gap-1.5 border-b px-2 py-1.5'>
        <Code className='h-3.5 w-3.5 shrink-0 text-muted-foreground' aria-hidden='true' />
        <span className='font-mono text-[10px] text-muted-foreground'>Transform</span>
        <span className='truncate font-medium'>{kind.label}</span>
        <NodeMenuButton nodeId={id} label={kind.label} />
      </div>

      <NodeStatusCaption
        status={status}
        skipReason={data.skipReason}
        error={data.error}
        progress={data.progress}
      />
      <DurationChip durationMs={data.durationMs} />

      <div className='px-2 py-1.5'>
        <span className='text-muted-foreground'>script</span>
        <p
          data-testid='transform-script-preview'
          title={firstLine}
          className={cn('truncate font-mono', preview === null && 'text-muted-foreground')}
        >
          {preview ?? '(empty)'}
        </p>
      </div>

      <div className='relative flex justify-end px-2 pb-1.5 pr-4'>
        <span className='text-muted-foreground'>result</span>
        <Handle
          type='source'
          id={RESULT_HANDLE}
          position={Position.Right}
          isConnectable={isConnectable}
          className='!h-2 !w-2'
        />
      </div>
    </div>
  );
}
