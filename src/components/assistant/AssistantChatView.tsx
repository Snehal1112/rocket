import { Check, Loader2, X } from 'lucide-react';
import { useEffect, useMemo, useRef } from 'react';
import { MarkdownRenderer } from '@/components/collections/MarkdownRenderer';
import { ScrollArea } from '@/components/ui/scroll-area';
import { buildTimeline } from '@/lib/assistant/timeline';
import { cn } from '@/lib/utils';
import {
  type AssistantMessage,
  type ToolActivityStatus,
  useAssistantStore,
} from '@/stores/assistant-store';
import { AssistantProposalCard } from './AssistantProposalCard';

/** How close to the bottom, in pixels, still counts as following the chat. */
const PIN_THRESHOLD_PX = 48;

const TOOL_STATUS_LABEL: Record<ToolActivityStatus, string> = {
  pending: 'waiting',
  in_progress: 'running',
  completed: 'done',
  failed: 'failed',
};

function ToolActivityLine({ title, status }: { title: string; status: ToolActivityStatus }) {
  const Icon = status === 'completed' ? Check : status === 'failed' ? X : Loader2;
  const busy = status === 'pending' || status === 'in_progress';
  return (
    <div className='flex items-center gap-1.5 text-xs text-muted-foreground'>
      <Icon
        className={cn(
          'h-3 w-3 shrink-0',
          busy && 'animate-spin',
          status === 'failed' && 'text-destructive',
        )}
        aria-hidden='true'
      />
      <span className='truncate'>{title}</span>
      <span className='sr-only'>{TOOL_STATUS_LABEL[status]}</span>
    </div>
  );
}

export function ChatItem({
  message,
  isLastStreaming,
}: {
  message: AssistantMessage;
  isLastStreaming: boolean;
}) {
  switch (message.kind) {
    case 'user':
      return (
        <div className='text-sm'>
          <div className='mb-1 text-xs font-semibold text-muted-foreground'>You</div>
          <p className='whitespace-pre-wrap'>{message.text}</p>
        </div>
      );
    case 'agent': {
      // Only the last streaming segment shows the spinner.
      const waiting = message.streaming && isLastStreaming;
      // A reply segment that got no text before a tool call adds nothing.
      if (!message.text && !message.error && !waiting) return null;
      return (
        <div className='text-sm'>
          <div className='mb-1 text-xs font-semibold text-muted-foreground'>Assistant</div>
          {message.text && <MarkdownRenderer restricted>{message.text}</MarkdownRenderer>}
          {waiting && (
            <Loader2
              className='h-3 w-3 animate-spin text-muted-foreground'
              aria-label='Assistant is replying'
            />
          )}
          {message.error && <p className='text-xs text-destructive'>{message.error}</p>}
        </div>
      );
    }
    case 'tool':
      return <ToolActivityLine title={message.title} status={message.status} />;
    case 'notice':
      return <p className='text-xs italic text-muted-foreground'>{message.text}</p>;
  }
}

/** The conversation: messages, tool lines, notices and resolved proposals. */
export function AssistantChatView() {
  const messages = useAssistantStore((s) => s.messages);
  const proposals = useAssistantStore((s) => s.proposals);
  const anchors = useAssistantStore((s) => s.proposalAnchors);
  const endRef = useRef<HTMLDivElement>(null);
  const pending = useMemo(() => proposals.filter((p) => p.status === 'pending'), [proposals]);
  const timeline = useMemo(
    () =>
      buildTimeline(
        messages,
        proposals.filter((p) => p.status !== 'pending'),
        anchors,
      ),
    [messages, proposals, anchors],
  );
  const itemCount = timeline.length + pending.length;

  // Changes when an item is added or the last message grows while streaming.
  const lastMessage = messages[messages.length - 1];
  const tailLength = lastMessage && 'text' in lastMessage ? lastMessage.text.length : 0;
  const userCount = messages.filter((m) => m.kind === 'user').length;
  const seenUserCount = useRef(userCount);
  const pinnedRef = useRef(true);

  // Tracks whether the reader sits at the bottom of the scroll viewport.
  useEffect(() => {
    const viewport = endRef.current?.closest('[data-radix-scroll-area-viewport]');
    if (!viewport) return;
    const onScroll = () => {
      const distance = viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight;
      pinnedRef.current = distance <= PIN_THRESHOLD_PX;
    };
    viewport.addEventListener('scroll', onScroll, { passive: true });
    return () => viewport.removeEventListener('scroll', onScroll);
  }, []);

  // Keeps the newest content in view unless the reader scrolled up.
  // biome-ignore lint/correctness/useExhaustiveDependencies: itemCount and tailLength are change triggers only.
  useEffect(() => {
    if (itemCount === 0) return;
    // Sending a message always brings the reader back to the bottom.
    if (userCount !== seenUserCount.current) {
      seenUserCount.current = userCount;
      pinnedRef.current = true;
    }
    if (!pinnedRef.current) return;
    endRef.current?.scrollIntoView?.({ block: 'end' });
  }, [itemCount, tailLength, userCount]);

  let lastStreamingId: string | undefined;
  for (const m of messages) {
    if (m.kind === 'agent' && m.streaming) lastStreamingId = m.id;
  }
  return (
    <ScrollArea className='min-h-0 flex-1'>
      <div className='flex flex-col gap-3 p-3'>
        {timeline.map((entry) =>
          entry.type === 'message' ? (
            <ChatItem
              key={entry.message.id}
              message={entry.message}
              isLastStreaming={entry.message.id === lastStreamingId}
            />
          ) : (
            <AssistantProposalCard key={entry.proposal.id} proposal={entry.proposal} inline />
          ),
        )}
        {pending.length > 0 && (
          <section aria-label='Proposals' className='flex flex-col gap-2'>
            {pending.map((p) => (
              <AssistantProposalCard key={p.id} proposal={p} />
            ))}
          </section>
        )}
        <div ref={endRef} />
      </div>
    </ScrollArea>
  );
}
