import { Handle, type NodeProps, Position } from '@xyflow/react';
import { MoreVertical } from 'lucide-react';
import type { FlowNodeKind, FlowNodeStatus } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';

export interface RequestNodeData {
  kind: Extract<FlowNodeKind, { kind: 'Request' }>;
  status: FlowNodeStatus;
  statusCode?: number;
  durationMs?: number;
  error?: string;
  headerCount?: number;
  bodyPreview?: string;
}

const METHOD_FROM_SOURCE = (kind: RequestNodeData['kind']) =>
  kind.source.type === 'Inline' ? kind.source.request.method : 'GET';
// Saved sources don't carry their method on the node itself (it lives in the
// referenced request file, resolved server-side at run time) — v1 shows a
// generic method badge for Saved nodes until Plan 10's sidebar-drag flow
// optionally hydrates a cached method label. Not a gap in this task: the
// spec's Saved/Inline distinction (§4) never promises client-visible method
// for Saved without an extra read, and no task in this plan claims to add one.

const statusStyles: Record<FlowNodeStatus, string> = {
  idle: 'border-border',
  running: 'border-blue-400 shadow-[0_0_0_1px_rgba(96,165,250,0.5)] animate-pulse',
  success: 'border-green-500 shadow-[0_0_0_1px_rgba(34,197,94,0.5)]',
  failed: 'border-red-500 shadow-[0_0_0_1px_rgba(239,68,68,0.5)]',
  skipped: 'border-muted-foreground/40 opacity-60',
};

export function RequestNode({ data, isConnectable }: NodeProps & { data: RequestNodeData }) {
  const { kind, status, statusCode, durationMs, error } = data;
  const method = METHOD_FROM_SOURCE(kind);
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
        statusStyles[status],
      )}
    >
      <div className='flex items-center justify-between gap-2 border-b px-2 py-1.5'>
        <div className='flex items-center gap-1.5 truncate'>
          <span className='rounded bg-muted px-1 py-0.5 font-mono text-[10px]'>{method}</span>
          <span className='truncate font-medium'>{kind.label}</span>
        </div>
        <MoreVertical className='h-3.5 w-3.5 shrink-0 text-muted-foreground' aria-hidden='true' />
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

      <div className='relative space-y-1 px-2 py-1.5'>
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
        id='result'
        position={Position.Right}
        isConnectable={isConnectable}
        className='!h-2 !w-2'
      />
    </div>
  );
}
