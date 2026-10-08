import { Handle, type NodeProps, Position } from '@xyflow/react';
import { Check, Copy } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { TRIGGER_HANDLE } from '@/lib/flow-handles';
import type { FlowIssue } from '@/lib/flow-issues';
import { formatOutputValue } from '@/lib/flow-output';
import type { FlowNodeKind, FlowNodeStatus, FlowSkipReason } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { DurationChip } from './DurationChip';
import { NodeIssueBadge } from './NodeIssueBadge';
import { NodeMenuButton } from './NodeMenuButton';
import { NodeStatusCaption } from './NodeStatusCaption';
import { issueRingClassName, nodeStatusClassName } from './nodeStatus';

export interface OutputNodeData {
  kind: Extract<FlowNodeKind, { kind: 'Output' }>;
  status: FlowNodeStatus;
  skipReason?: FlowSkipReason;
  error?: string;
  /** Progress text while running, such as "attempt 3/30". */
  progress?: string;
  /** How long the last run of this node took. */
  durationMs?: number;
  /** Problems found in this node, drawn as a ring and a badge. */
  issues?: FlowIssue[];
  value?: string;
  /** False when no wire feeds `value`, so an empty value is not the wire's result. */
  hasValueWire?: boolean;
}

const COPIED_MS = 1500;

export function OutputNode({ id, data, isConnectable }: NodeProps & { data: OutputNodeData }) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  // Clear the pending reset so it cannot fire after unmount.
  useEffect(() => () => clearTimeout(timer.current), []);

  const value = data.value;

  const copy = () => {
    if (!value) return;
    navigator.clipboard.writeText(value).then(
      () => {
        setCopied(true);
        clearTimeout(timer.current);
        timer.current = setTimeout(() => setCopied(false), COPIED_MS);
      },
      (err) => console.warn('Copy failed', err),
    );
  };

  return (
    <div
      data-testid='output-node-card'
      data-status={data.status}
      className={cn(
        'w-max min-w-48 max-w-[28rem] rounded-md border bg-card text-card-foreground text-xs shadow-sm',
        nodeStatusClassName(data.status, data.skipReason),
        issueRingClassName(data.issues),
      )}
    >
      <div className='flex items-center gap-1.5 border-b px-2 py-1.5 font-medium'>
        <span className='truncate'>{data.kind.label}</span>
        <NodeIssueBadge issues={data.issues} />
        <NodeMenuButton nodeId={id} label={data.kind.label} />
      </div>
      <NodeStatusCaption
        status={data.status}
        skipReason={data.skipReason}
        error={data.error}
        progress={data.progress}
      />
      <DurationChip durationMs={data.durationMs} />
      {/* Each input sits in a labelled row, like the Request node, so the
          data-less "Run when" gate is not mistaken for the `value` input. */}
      <div className='relative space-y-1 px-2 pt-1.5'>
        <div
          data-testid='output-node-trigger-row'
          className='relative flex items-center gap-1.5 pl-2'
        >
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
          data-testid='output-node-value-row'
          className='relative flex items-center gap-1.5 pl-2'
        >
          <Handle
            type='target'
            id='value'
            position={Position.Left}
            isConnectable={isConnectable}
            className='!h-2 !w-2'
          />
          <span className='text-muted-foreground'>Value</span>
        </div>
      </div>
      <div className='flex items-start gap-1 px-2 py-1.5 text-muted-foreground'>
        {value === undefined ? (
          <span>—</span>
        ) : (
          <pre
            data-testid='output-node-value'
            className='nowheel nodrag nokey max-h-80 min-w-0 flex-1 select-text overflow-auto whitespace-pre-wrap font-mono text-[11px] [overflow-wrap:anywhere]'
          >
            {value === '' ? (
              <span className='italic'>
                {data.hasValueWire === false ? '(no value wired)' : '(empty)'}
              </span>
            ) : (
              formatOutputValue(value)
            )}
          </pre>
        )}
        {value ? (
          <Button
            type='button'
            variant='ghost'
            size='icon'
            className='nodrag nokey h-5 w-5 shrink-0'
            aria-label='Copy value'
            title='Copy value'
            onClick={copy}
          >
            {copied ? <Check className='h-3 w-3' /> : <Copy className='h-3 w-3' />}
          </Button>
        ) : null}
      </div>
    </div>
  );
}
