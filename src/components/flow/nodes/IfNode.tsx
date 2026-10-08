import { Handle, type NodeProps, Position } from '@xyflow/react';
import { GitBranch } from 'lucide-react';
import { SingleLineEditor } from '@/components/editor';
import { Badge } from '@/components/ui/badge';
import { FALSE_HANDLE, INPUT_HANDLE, TRUE_HANDLE } from '@/lib/flow-handles';
import type { FlowIssue } from '@/lib/flow-issues';
import type { FlowNodeKind, FlowNodeStatus, FlowSkipReason } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { exitLabel } from '../flowExits';
import { DurationChip } from './DurationChip';
import { useFlowNodeActions } from './FlowNodeActionsContext';
import { NodeIssueBadge } from './NodeIssueBadge';
import { NodeMenuButton } from './NodeMenuButton';
import { NodeStatusCaption } from './NodeStatusCaption';
import { NodeStatusIcon } from './NodeStatusIcon';
import { issueRingClassName, nodeStatusClassName } from './nodeStatus';

export type IfNodeData = {
  kind: Extract<FlowNodeKind, { kind: 'If' }>;
  status: FlowNodeStatus;
  error?: string;
  /** Progress text while running, such as "attempt 3/30". */
  progress?: string;
  /** True when this result is from an earlier run than the tab's last run. */
  cached?: boolean;
  /** How long the last run of this node took. */
  durationMs?: number;
  skipReason?: FlowSkipReason;
  /** Exit chosen by the last run: "true" or "false". */
  branch?: string;
  /** Problems found in this node, drawn as a ring and a badge. */
  issues?: FlowIssue[];
};

export function IfNode({ id, data, isConnectable }: NodeProps & { data: IfNodeData }) {
  const { updateNodeKind } = useFlowNodeActions();
  const { kind, status } = data;

  return (
    <div
      data-testid='if-node-card'
      data-status={status}
      className={cn(
        'w-64 rounded-md border bg-card text-card-foreground text-xs shadow-sm',
        nodeStatusClassName(status, data.skipReason),
        issueRingClassName(data.issues),
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
        <GitBranch className='h-3.5 w-3.5 shrink-0 text-muted-foreground' aria-hidden='true' />
        <span className='font-mono text-[10px] text-muted-foreground'>If</span>
        <span className='truncate font-medium'>{kind.label}</span>
        <NodeIssueBadge issues={data.issues} />
        <NodeStatusIcon status={data.status} />
        <NodeMenuButton nodeId={id} label={kind.label} />
      </div>

      {status === 'success' && data.branch && (
        <div className='px-2 pt-1'>
          <Badge variant='secondary' data-testid='branch-badge'>
            → {exitLabel(kind, data.branch) ?? data.branch}
          </Badge>
        </div>
      )}
      <NodeStatusCaption
        status={status}
        skipReason={data.skipReason}
        error={data.error}
        progress={data.progress}
        cached={data.cached}
      />
      <DurationChip durationMs={data.durationMs} />

      {/* nodrag/nowheel/nokey keep typing, selecting text and scrolling in
          the editor from dragging the node or deleting it on Backspace. */}
      <div className='nodrag nowheel nokey px-2 py-1.5'>
        <span className='text-muted-foreground'>condition</span>
        <SingleLineEditor
          aria-label='Condition'
          value={kind.condition}
          onChange={(condition) => updateNodeKind(id, { ...kind, condition })}
          placeholder='response.status === 200'
          className='text-xs'
        />
      </div>

      <div className='space-y-1 px-2 pb-1.5'>
        <div className='relative flex justify-end pr-2'>
          <span className='text-green-600'>true</span>
          <Handle
            type='source'
            id={TRUE_HANDLE}
            position={Position.Right}
            isConnectable={isConnectable}
            className='!h-2 !w-2 !bg-green-500'
          />
        </div>
        <div className='relative flex justify-end pr-2'>
          <span className='text-muted-foreground'>false</span>
          <Handle
            type='source'
            id={FALSE_HANDLE}
            position={Position.Right}
            isConnectable={isConnectable}
            className='!h-2 !w-2 !bg-muted-foreground'
          />
        </div>
      </div>
    </div>
  );
}
