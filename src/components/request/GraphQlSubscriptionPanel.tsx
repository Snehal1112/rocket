import { SingleLineEditor } from '@/components/editor';
import { Badge } from '@/components/ui/badge';
import { Label } from '@/components/ui/label';
import { cn } from '@/lib/utils';
import { IDLE_SESSION, useWebSocketStore } from '@/stores/websocket-store';
import type { RequestTab } from '@/types/pane-types';
import { MessageLog } from './websocket/MessageLog';

const STATUS_LABEL = {
  idle: 'Not subscribed',
  connecting: 'Connecting',
  open: 'Subscribed',
  closed: 'Ended',
  failed: 'Failed',
} as const;

interface GraphQlSubscriptionPanelProps {
  tab: RequestTab;
  onConnectionParamsChange: (text: string) => void;
}

// The response area of a GraphQL tab whose operation is a subscription: connection params, the
// subscription status and the live stream of results.
export function GraphQlSubscriptionPanel({
  tab,
  onConnectionParamsChange,
}: GraphQlSubscriptionPanelProps) {
  const session = useWebSocketStore((s) => s.byTab[tab.id]) ?? IDLE_SESSION;
  const clearLog = useWebSocketStore((s) => s.clearLog);

  return (
    <div className='flex h-full min-h-0 flex-col'>
      <div className='flex flex-wrap items-center gap-2 border-b px-3 py-2'>
        <Badge
          variant='outline'
          className={cn(
            session.status === 'open' && 'text-[hsl(var(--success))]',
            session.status === 'failed' && 'text-destructive',
          )}
        >
          {STATUS_LABEL[session.status]}
        </Badge>
        {session.subprotocol && (
          <Badge variant='secondary' className='font-mono text-[10px]'>
            {session.subprotocol}
          </Badge>
        )}
        {session.status === 'failed' && session.error && (
          <span className='text-xs text-destructive'>{session.error}</span>
        )}
        <div className='ml-auto flex min-w-[220px] flex-1 items-center gap-2 sm:max-w-md'>
          <Label className='shrink-0 text-xs text-muted-foreground'>Connection params</Label>
          <SingleLineEditor
            value={tab.request.graphql?.connectionParams ?? ''}
            onChange={onConnectionParamsChange}
            placeholder='{"authToken": "{{token}}"}'
            className='flex-1'
          />
        </div>
      </div>
      <div className='min-h-0 flex-1'>
        <MessageLog
          entries={session.log}
          onClear={() => clearLog(tab.id)}
          title='Results'
          emptyText='Subscribe to see streamed results here.'
        />
      </div>
    </div>
  );
}
