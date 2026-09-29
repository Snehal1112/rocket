import { Handle, type NodeProps, Position } from '@xyflow/react';
import { Check, Copy } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { TRIGGER_HANDLE } from '@/lib/flow-handles';
import { formatOutputValue } from '@/lib/flow-output';
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
        style={{ top: 30 }}
        className='!h-2 !w-2'
      />
      <div className='flex items-center gap-1.5 border-b px-2 py-1.5 font-medium'>
        <span className='truncate'>{data.kind.label}</span>
        <NodeMenuButton nodeId={id} label={data.kind.label} />
      </div>
      <NodeStatusCaption status={data.status} skipReason={data.skipReason} error={data.error} />
      <div className='flex items-start gap-1 px-2 py-1.5 text-muted-foreground'>
        {value === undefined ? (
          <span>—</span>
        ) : (
          <pre
            data-testid='output-node-value'
            className='nowheel nodrag nokey max-h-80 min-w-0 flex-1 select-text overflow-auto whitespace-pre-wrap font-mono text-[11px] [overflow-wrap:anywhere]'
          >
            {value === '' ? <span className='italic'>(empty)</span> : formatOutputValue(value)}
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
