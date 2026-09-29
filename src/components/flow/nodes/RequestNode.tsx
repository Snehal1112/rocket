import { Handle, type NodeProps, Position } from '@xyflow/react';
import { Bug, Repeat } from 'lucide-react';
import { RESULT_HANDLE, TRIGGER_HANDLE } from '@/lib/flow-handles';
import { msToSecondsLabel } from '@/lib/flow-repeat';
import type { FlowNodeKind, FlowNodeStatus, FlowSkipReason } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { useFlowNodeActions } from './FlowNodeActionsContext';
import { NodeMenuButton } from './NodeMenuButton';
import { NodeStatusCaption } from './NodeStatusCaption';
import { nodeStatusClassName } from './nodeStatus';

export interface RequestNodeData {
  kind: Extract<FlowNodeKind, { kind: 'Request' }>;
  status: FlowNodeStatus;
  skipReason?: FlowSkipReason;
  statusCode?: number;
  durationMs?: number;
  error?: string;
  /** Progress text while running, such as "attempt 3/30". */
  progress?: string;
  /** Attempts a repeat-until run made. Set after a run. */
  attempts?: number;
  headerCount?: number;
  bodyPreview?: string;
  /** Method of a Saved request, when the caller has looked it up. */
  method?: string;
  /** Set when this node is named in a save validation error, such as a cycle. */
  hasCycleError?: boolean;
}

// A Saved source stores only its request path, not its method. The method
// lives in the referenced request file. Show it only when the caller passes
// `data.method`, and a neutral "SAVED" badge otherwise, never a guessed GET.
const methodLabel = (data: RequestNodeData) =>
  data.kind.source.type === 'Inline' ? data.kind.source.request.method : (data.method ?? 'SAVED');

const attemptsLabel = (attempts: number) =>
  `${attempts} ${attempts === 1 ? 'attempt' : 'attempts'}`;

export function RequestNode({ id, data, isConnectable }: NodeProps & { data: RequestNodeData }) {
  const { kind, status, statusCode, durationMs, error } = data;
  const { updateNodeKind } = useFlowNodeActions();
  const method = methodLabel(data);
  const url = kind.source.type === 'Inline' ? kind.source.request.url : kind.source.requestPath;
  const headerCount =
    kind.source.type === 'Inline' ? kind.source.request.headers.length : (data.headerCount ?? 0);
  const bodyPreview =
    kind.source.type === 'Inline' ? (kind.source.request.body ?? '—') : (data.bodyPreview ?? '—');

  return (
    <div
      data-testid='request-node-card'
      data-status={status}
      className={cn(
        'w-64 rounded-md border bg-card text-card-foreground text-xs shadow-sm',
        nodeStatusClassName(status, data.skipReason),
        data.hasCycleError && 'ring-2 ring-red-500',
      )}
    >
      <div className='flex items-center justify-between gap-2 border-b px-2 py-1.5'>
        <div className='flex items-center gap-1.5 truncate'>
          <span className='rounded bg-muted px-1 py-0.5 font-mono text-[10px]'>{method}</span>
          <span className='truncate font-medium'>{kind.label}</span>
        </div>
        {kind.debug && (
          <Bug
            data-testid='request-node-debug-badge'
            aria-label='Debug mode on'
            role='img'
            className='h-3.5 w-3.5 shrink-0 text-amber-500'
          />
        )}
        <NodeMenuButton
          nodeId={id}
          label={kind.label}
          debug={{
            enabled: kind.debug === true,
            onToggle: (enabled) => updateNodeKind(id, { ...kind, debug: enabled }),
          }}
        />
      </div>

      {status === 'success' && (
        <div className='px-2 pt-1 text-green-600'>
          {data.attempts === undefined
            ? `✓ ${statusCode} · ${durationMs}ms`
            : `✓ ${statusCode} · ${attemptsLabel(data.attempts)} · ${msToSecondsLabel(durationMs ?? 0)}`}
        </div>
      )}
      {/* A failed poll also shows how many attempts it made. */}
      {status === 'failed' && (
        <div className='px-2 pt-1 text-red-600'>
          {data.attempts === undefined
            ? `✕ ${statusCode ?? 'Error'} · ${error ?? `${durationMs}ms`}`
            : `✕ ${statusCode ?? 'Error'} · ${attemptsLabel(data.attempts)} · ${error ?? msToSecondsLabel(durationMs ?? 0)}`}
        </div>
      )}
      {/* The line above already shows a failure, so the caption covers skips only. */}
      {status !== 'failed' && (
        <NodeStatusCaption status={status} skipReason={data.skipReason} progress={data.progress} />
      )}

      {/* Every field row, including the data-less "Run when" trigger row, is
          always rendered, even when empty, so each target handle stays
          connectable. There is one `headers` handle for all
          header slots. Plan 10's connection UI picks the header by name and
          writes a `headers[<name>].value` target field. */}
      <div className='relative space-y-1 px-2 py-1.5'>
        <div className='relative flex items-center gap-1.5 pl-2'>
          <Handle
            type='target'
            id={TRIGGER_HANDLE}
            position={Position.Left}
            isConnectable={isConnectable}
            className='!h-2 !w-2'
          />
          <span className='text-muted-foreground'>Run when</span>
        </div>
        <div className='relative flex items-center gap-1.5 pl-2'>
          <Handle
            type='target'
            id='url'
            position={Position.Left}
            isConnectable={isConnectable}
            className='!h-2 !w-2'
          />
          <span className='text-muted-foreground'>URL</span>
          <span className='truncate'>{url}</span>
        </div>
        <div
          data-testid='request-node-headers-row'
          className='relative flex items-center gap-1.5 pl-2'
        >
          <Handle
            type='target'
            id='headers'
            position={Position.Left}
            isConnectable={isConnectable}
            className='!h-2 !w-2'
          />
          <span className='text-muted-foreground'>Headers</span>
          <span>{headerCount} set</span>
        </div>
        <div className='relative flex items-center gap-1.5 pl-2'>
          <Handle
            type='target'
            id='body'
            position={Position.Left}
            isConnectable={isConnectable}
            className='!h-2 !w-2'
          />
          <span className='text-muted-foreground'>Body</span>
          <span className='truncate'>{bodyPreview}</span>
        </div>
        {/* Repeat until has no handle: it is a setting, not an input. */}
        {kind.repeatUntil && (
          <div
            data-testid='request-node-repeat-row'
            title={kind.repeatUntil.condition}
            className='flex min-w-0 items-center gap-1.5 pl-2 text-muted-foreground'
          >
            <Repeat className='h-3 w-3 shrink-0' aria-hidden='true' />
            <span className='truncate'>
              until {kind.repeatUntil.condition} · {msToSecondsLabel(kind.repeatUntil.intervalMs)} ·
              max {kind.repeatUntil.maxAttempts}
            </span>
          </div>
        )}
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
