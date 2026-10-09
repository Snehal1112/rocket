import { Check, Loader2, X } from 'lucide-react';
import { useEffect, useRef } from 'react';
import { MarkdownRenderer } from '@/components/collections/MarkdownRenderer';
import { ScrollArea } from '@/components/ui/scroll-area';
import { cn } from '@/lib/utils';
import {
  type AssistantMessage,
  type ToolActivityStatus,
  useAssistantStore,
} from '@/stores/assistant-store';
import { AssistantProposalCard } from './AssistantProposalCard';

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

function ChatItem({
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

/** The conversation: messages, tool activity lines, notices and proposals. */
export function AssistantChatView() {
  const messages = useAssistantStore((s) => s.messages);
  const proposals = useAssistantStore((s) => s.proposals);
  const endRef = useRef<HTMLDivElement>(null);
  const itemCount = messages.length + proposals.length;

  // Keeps the newest item in view.
  useEffect(() => {
    if (itemCount === 0) return;
    endRef.current?.scrollIntoView?.({ block: 'end' });
  }, [itemCount]);

  let lastStreamingId: string | undefined;
  for (const m of messages) {
    if (m.kind === 'agent' && m.streaming) lastStreamingId = m.id;
  }
  return (
    <ScrollArea className='min-h-0 flex-1'>
      <div className='flex flex-col gap-3 p-3'>
        {messages.map((m) => (
          <ChatItem key={m.id} message={m} isLastStreaming={m.id === lastStreamingId} />
        ))}
        {proposals.length > 0 && (
          <section aria-label='Proposals' className='flex flex-col gap-2'>
            {proposals.map((p) => (
              <AssistantProposalCard key={p.id} proposal={p} />
            ))}
          </section>
        )}
        <div ref={endRef} />
      </div>
    </ScrollArea>
  );
}
