import { Loader2 } from 'lucide-react';
import { useEffect, useMemo, useRef } from 'react';
import { MarkdownRenderer } from '@/components/collections/MarkdownRenderer';
import { ScrollArea } from '@/components/ui/scroll-area';
import { buildTimeline, groupToolRuns } from '@/lib/assistant/timeline';
import { type AssistantMessage, useAssistantStore } from '@/stores/assistant-store';
import { AssistantProposalCard } from './AssistantProposalCard';
import { ReasoningBlock } from './ReasoningBlock';
import { ToolRun } from './ToolRun';
import { WaitingNotice } from './WaitingNotice';

/** How close to the bottom, in pixels, still counts as following the chat. */
const PIN_THRESHOLD_PX = 48;

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
        <div className='min-w-0 whitespace-pre-wrap break-words rounded-lg bg-muted/60 px-3 py-2 text-sm'>
          <span className='sr-only'>You: </span>
          {message.text}
        </div>
      );
    case 'agent': {
      // Only the last streaming segment shows the spinner.
      const waiting = message.streaming && isLastStreaming;
      // A reply segment that got no text before a tool call adds nothing.
      if (!message.text && !message.error && !waiting) return null;
      return (
        <div className='min-w-0 break-words text-sm leading-relaxed'>
          <span className='sr-only'>Assistant: </span>
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
      // A single tool message is shown as a run of one.
      return <ToolRun tools={[message]} />;
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
      groupToolRuns(
        buildTimeline(
          messages,
          proposals.filter((p) => p.status !== 'pending'),
          anchors,
        ),
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
      <div className='flex min-w-0 flex-col gap-5 px-4 py-4'>
        {timeline.map((entry) => {
          if (entry.type === 'tools') return <ToolRun key={entry.id} tools={entry.tools} />;
          if (entry.type === 'proposal') {
            return (
              <AssistantProposalCard key={entry.proposal.id} proposal={entry.proposal} inline />
            );
          }
          return (
            <ChatItem
              key={entry.message.id}
              message={entry.message}
              isLastStreaming={entry.message.id === lastStreamingId}
            />
          );
        })}
        <ReasoningBlock />
        <WaitingNotice />
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
