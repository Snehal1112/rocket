import { ArrowDownLeft, ArrowUpRight, Info, Trash2 } from 'lucide-react';
import { useEffect, useRef } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { formatSize, formatTime, previewPayload } from '@/lib/message-log-format';
import { cn } from '@/lib/utils';
import type { MessageDirection, MessageLogEntry } from '@/types/message-log';

interface MessageLogProps {
  entries: MessageLogEntry[];
  onClear: () => void;
  title?: string;
  emptyText?: string;
}

const DIRECTION_LABEL: Record<MessageDirection, string> = {
  in: 'Received',
  out: 'Sent',
  system: 'Status',
};

function DirectionIcon({ direction }: { direction: MessageDirection }) {
  const className = cn(
    'h-3.5 w-3.5 shrink-0 mt-0.5',
    direction === 'in' && 'text-[hsl(var(--success))]',
    direction === 'out' && 'text-primary',
    direction === 'system' && 'text-muted-foreground',
  );
  if (direction === 'in') return <ArrowDownLeft aria-hidden='true' className={className} />;
  if (direction === 'out') return <ArrowUpRight aria-hidden='true' className={className} />;
  return <Info aria-hidden='true' className={className} />;
}

/**
 * A live, auto-scrolling message log. Generic on purpose: the WebSocket tab and the GraphQL
 * subscription panel both render it. It follows new entries only while the user is at the bottom.
 */
export function MessageLog({
  entries,
  onClear,
  title = 'Messages',
  emptyText = 'No messages yet.',
}: MessageLogProps) {
  const scroller = useRef<HTMLDivElement>(null);
  const stickToBottom = useRef(true);

  // biome-ignore lint/correctness/useExhaustiveDependencies: scroll when the entry count changes.
  useEffect(() => {
    const el = scroller.current;
    if (el && stickToBottom.current) el.scrollTop = el.scrollHeight;
  }, [entries.length]);

  const handleScroll = () => {
    const el = scroller.current;
    if (!el) return;
    stickToBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
  };

  return (
    <div className='flex h-full min-h-0 flex-col'>
      <div className='flex items-center justify-between border-b px-3 py-1.5'>
        <span className='text-xs font-medium text-muted-foreground'>
          {title} ({entries.length})
        </span>
        <Button
          size='icon'
          variant='ghost'
          className='h-6 w-6'
          aria-label='Clear log'
          onClick={onClear}
          disabled={entries.length === 0}
        >
          <Trash2 aria-hidden='true' className='h-3.5 w-3.5' />
        </Button>
      </div>
      <div ref={scroller} onScroll={handleScroll} className='min-h-0 flex-1 overflow-auto'>
        {entries.length === 0 ? (
          <p className='px-3 py-4 text-xs text-muted-foreground'>{emptyText}</p>
        ) : (
          <ul>
            {entries.map((entry) => (
              <li
                key={entry.id}
                data-direction={entry.direction}
                className='flex items-start gap-2 border-b border-border/50 px-3 py-1 font-mono text-xs'
              >
                <span title={DIRECTION_LABEL[entry.direction]}>
                  <DirectionIcon direction={entry.direction} />
                </span>
                <span className='shrink-0 text-muted-foreground'>
                  {formatTime(entry.timestampMs)}
                </span>
                {entry.label && (
                  <Badge variant='outline' className='shrink-0 px-1.5 py-0 text-[10px]'>
                    {entry.label}
                  </Badge>
                )}
                {entry.direction !== 'system' && (
                  <span className='shrink-0 text-muted-foreground'>{formatSize(entry.size)}</span>
                )}
                <pre
                  className={cn(
                    'min-w-0 flex-1 whitespace-pre-wrap break-all font-mono',
                    entry.direction === 'system' && 'italic text-muted-foreground',
                  )}
                >
                  {previewPayload(entry)}
                </pre>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
