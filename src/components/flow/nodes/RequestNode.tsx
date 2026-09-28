import { Handle, type NodeProps, Position } from '@xyflow/react';
import { RESULT_HANDLE, TRIGGER_HANDLE } from '@/lib/flow-handles';
import type { FlowNodeKind, FlowNodeStatus, FlowSkipReason } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
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

export function RequestNode({ id, data, isConnectable }: NodeProps & { data: RequestNodeData }) {
  const { kind, status, statusCode, durationMs, error } = data;
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
        <NodeMenuButton nodeId={id} label={kind.label} />
      </div>

      {status === 'success' && (
        <div className='px-2 pt-1 text-green-600'>
          ✓ {statusCode} · {durationMs}ms
        </div>
      )}
      {status === 'failed' && (
        <div className='px-2 pt-1 text-red-600'>
          ✕ {statusCode ?? 'Error'} · {error ?? `${durationMs}ms`}
        </div>
      )}
      <NodeStatusCaption status={status} skipReason={data.skipReason} />

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
