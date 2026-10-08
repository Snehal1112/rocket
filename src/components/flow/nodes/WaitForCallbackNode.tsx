import { Handle, type NodeProps, Position } from '@xyflow/react';
import { Check, Copy, Hourglass } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { callbackVariable } from '@/lib/flow-callback';
import { RESULT_HANDLE, TRIGGER_HANDLE } from '@/lib/flow-handles';
import type { FlowIssue } from '@/lib/flow-issues';
import type { FlowNodeKind, FlowNodeStatus, FlowSkipReason } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { CallbackUrlField } from '../CallbackUrlField';
import { NodeIssueBadge } from './NodeIssueBadge';
import { NodeMenuButton } from './NodeMenuButton';
import { NodeStatusCaption } from './NodeStatusCaption';
import { NodeStatusIcon } from './NodeStatusIcon';
import { issueRingClassName, nodeStatusClassName } from './nodeStatus';

export interface WaitForCallbackNodeData {
  kind: Extract<FlowNodeKind, { kind: 'WaitForCallback' }>;
  status: FlowNodeStatus;
  skipReason?: FlowSkipReason;
  error?: string;
  /** The received method (e.g. `POST`) after a success. */
  value?: string;
  durationMs?: number;
  /** Live text while waiting, such as "waiting… 42s left". */
  progress?: string;
  /** True when this result is from an earlier run than the tab's last run. */
  cached?: boolean;
  /** This run's callback URL. Set only while the run is active. */
  callbackUrl?: string;
  /** Problems found in this node, drawn as a ring and a badge. */
  issues?: FlowIssue[];
}

const COPIED_MS = 1500;

export function WaitForCallbackNode({
  id,
  data,
  isConnectable,
}: NodeProps & { data: WaitForCallbackNodeData }) {
  const { kind, status } = data;
  const variable = callbackVariable(kind.name);
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  // Clear the pending reset so it cannot fire after unmount.
  useEffect(() => () => clearTimeout(timer.current), []);

  const copy = () => {
    navigator.clipboard.writeText(variable).then(
      () => {
        setCopied(true);
        clearTimeout(timer.current);
        timer.current = setTimeout(() => setCopied(false), COPIED_MS);
      },
      (err) => console.warn('Copy failed', err),
    );
  };

  const seconds = (ms: number) => `${Math.round(ms / 100) / 10}s`;
  const summary = `Wait for callback · ${kind.name} · ${Math.round(kind.timeoutMs / 1000)}s`;

  return (
    <div
      data-testid='wait-node-card'
      data-status={status}
      className={cn(
        'w-64 rounded-md border bg-card text-card-foreground text-xs shadow-sm',
        nodeStatusClassName(status, data.skipReason),
        issueRingClassName(data.issues),
      )}
    >
      <div className='flex items-center gap-1.5 border-b px-2 py-1.5'>
        <Hourglass className='h-3.5 w-3.5 shrink-0 text-muted-foreground' aria-hidden='true' />
        <span className='truncate font-medium'>{kind.label}</span>
        <NodeIssueBadge issues={data.issues} />
        <NodeStatusIcon status={data.status} />
        <NodeMenuButton nodeId={id} label={kind.label} isWait />
      </div>

      {status === 'success' && (
        <div data-testid='wait-node-result' className='px-2 pt-1 text-green-600'>
          ✓ received {data.value ?? 'call'}
          {data.durationMs !== undefined ? ` · ${seconds(data.durationMs)}` : ''}
        </div>
      )}
      <NodeStatusCaption
        status={status}
        skipReason={data.skipReason}
        error={data.error}
        progress={data.progress}
        cached={data.cached}
      />
      {data.callbackUrl && (
        <div className='px-2 pt-1'>
          <CallbackUrlField url={data.callbackUrl} />
        </div>
      )}

      <div className='relative space-y-1 px-2 py-1.5'>
        <div className='relative flex items-center gap-1.5 pl-2'>
          <Handle
            type='target'
            id={TRIGGER_HANDLE}
            title='Run when'
            position={Position.Left}
            isConnectable={isConnectable}
            className='!h-2 !w-2'
          />
          <span className='text-muted-foreground'>Run when</span>
        </div>
        <div
          data-testid='wait-node-summary'
          className='truncate text-muted-foreground'
          title={summary}
        >
          {summary}
        </div>
        <div className='nodrag nokey flex items-center gap-1'>
          <code data-testid='wait-node-variable' className='truncate font-mono text-[11px]'>
            {variable}
          </code>
          <Button
            type='button'
            variant='ghost'
            size='icon'
            className='h-5 w-5 shrink-0'
            aria-label='Copy variable'
            title='Copy variable'
            onClick={copy}
          >
            {copied ? <Check className='h-3 w-3' /> : <Copy className='h-3 w-3' />}
          </Button>
        </div>
      </div>

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
