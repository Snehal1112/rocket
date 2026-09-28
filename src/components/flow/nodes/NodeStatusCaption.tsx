import type { FlowNodeStatus, FlowSkipReason } from '@/lib/tauri-api';
import { nodeStatusCaption } from './nodeStatus';

// Shows why a node did not run normally: the error of a failed node, or the
// reason for a skipped one. Long errors wrap and stop at three lines.
export function NodeStatusCaption({
  status,
  skipReason,
  error,
}: {
  status: FlowNodeStatus;
  skipReason?: FlowSkipReason;
  error?: string;
}) {
  if (status === 'failed') {
    const message = error ?? 'Error';
    return (
      <div
        data-testid='node-error'
        title={message}
        className='line-clamp-3 break-words px-2 pt-1 text-red-600'
      >
        ✕ {message}
      </div>
    );
  }
  const caption = nodeStatusCaption(status, { skipReason });
  if (!caption) return null;
  return (
    <div data-testid='node-status-caption' className='px-2 pt-1 italic text-muted-foreground'>
      {caption}
    </div>
  );
}
