import type { FlowNodeStatus, FlowSkipReason } from '@/lib/tauri-api';
import { nodeStatusCaption } from './nodeStatus';

export function NodeStatusCaption({
  status,
  skipReason,
}: {
  status: FlowNodeStatus;
  skipReason?: FlowSkipReason;
}) {
  const caption = nodeStatusCaption(status, { skipReason });
  if (!caption) return null;
  return (
    <div data-testid='node-status-caption' className='px-2 pt-1 italic text-muted-foreground'>
      {caption}
    </div>
  );
}
