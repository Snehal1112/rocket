import { Handle, type NodeProps, Position } from '@xyflow/react';
import { KeyRound } from 'lucide-react';
import { describeAuth } from '@/lib/flow-auth';
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

export type AuthNodeData = {
  kind: Extract<FlowNodeKind, { kind: 'Auth' }>;
  status: FlowNodeStatus;
  error?: string;
  progress?: string;
  /** True when this result is from an earlier run than the tab's last run. */
  cached?: boolean;
  /** How long the last run of this node took. */
  durationMs?: number;
  skipReason?: FlowSkipReason;
  /** Problems found in this node, drawn as a ring and a badge. */
  issues?: FlowIssue[];
};

// An Auth node has no inputs. It shows only what kind of auth it holds, never
// a credential value.
export function AuthNode({ id, data, isConnectable }: NodeProps & { data: AuthNodeData }) {
  const { kind, status } = data;
  return (
    <div
      data-testid='auth-node-card'
      data-status={status}
      className={cn(
        'w-56 rounded-md border bg-card text-card-foreground text-xs shadow-sm',
        nodeStatusClassName(status, data.skipReason),
        issueRingClassName(data.issues),
      )}
    >
      <div className='flex items-center gap-1.5 border-b px-2 py-1.5'>
        <KeyRound className='h-3.5 w-3.5 shrink-0 text-muted-foreground' aria-hidden='true' />
        <span className='font-mono text-[10px] text-muted-foreground'>Auth</span>
        <span className='truncate font-medium'>{kind.label}</span>
        <NodeIssueBadge issues={data.issues} />
        <NodeStatusIcon status={data.status} />
        <NodeMenuButton nodeId={id} label={kind.label} />
      </div>

      <NodeStatusCaption
        status={status}
        skipReason={data.skipReason}
        error={data.error}
        progress={data.progress}
        cached={data.cached}
      />
      <DurationChip durationMs={data.durationMs} />

      <div className='space-y-0.5 px-2 py-1.5'>
        <p data-testid='auth-node-summary' className='truncate'>
          {describeAuth(kind.auth)}
        </p>
        {kind.applyToInherit && (
          <p data-testid='auth-node-applies' className='truncate text-muted-foreground'>
            Applies to inherited auth
          </p>
        )}
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
