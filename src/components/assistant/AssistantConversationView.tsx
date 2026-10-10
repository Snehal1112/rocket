import { ArrowLeft } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import type { AgentProposalStatus, ConversationRecord, StoredMessage } from '@/lib/tauri-api';
import type { AssistantMessage } from '@/stores/assistant-store';
import { ChatItem } from './AssistantChatView';

const PROPOSAL_STATUS_LABEL: Record<AgentProposalStatus, string> = {
  pending: 'not decided',
  accepted: 'accepted',
  rejected: 'rejected',
  stale: 'stale',
  failed: 'failed',
};

/** A stored message as a chat message for `ChatItem`. Stored messages have no id, so the index is used. */
export function toChatMessage(message: StoredMessage, index: number): AssistantMessage | undefined {
  const id = `stored-${index}`;
  switch (message.kind) {
    case 'user':
      return { kind: 'user', id, text: message.text };
    case 'agent':
      return { kind: 'agent', id, text: message.text, streaming: false };
    case 'tool':
      return { kind: 'tool', id, callId: id, title: message.title, status: message.status };
    case 'notice':
      return { kind: 'notice', id, text: message.text };
    case 'proposal':
      return undefined;
  }
}

/** One stored conversation, read-only. Proposals are history lines without actions. */
export function AssistantConversationView({
  record,
  agentLabel,
  onBack,
}: {
  record: ConversationRecord;
  agentLabel: string;
  onBack: () => void;
}) {
  return (
    <div className='flex min-h-0 flex-1 flex-col'>
      <div className='flex items-center gap-2 border-b px-3 py-2'>
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7'
          aria-label='Back to the history'
          onClick={onBack}
        >
          <ArrowLeft className='h-4 w-4' aria-hidden='true' />
        </Button>
        <div className='min-w-0 flex-1'>
          <p className='truncate text-sm font-medium'>{record.title || 'Untitled conversation'}</p>
          <p className='text-xs text-muted-foreground'>{`${agentLabel} · read-only`}</p>
        </div>
      </div>
      <ScrollArea className='min-h-0 flex-1'>
        <div className='flex flex-col gap-3 p-3'>
          {record.messages.map((message, index) => {
            const key = `stored-${index}`;
            if (message.kind === 'proposal') {
              return (
                <p key={key} className='text-xs text-muted-foreground'>
                  {`Proposal: ${message.summary} (${PROPOSAL_STATUS_LABEL[message.status]})`}
                </p>
              );
            }
            const chat = toChatMessage(message, index);
            return chat ? <ChatItem key={key} message={chat} isLastStreaming={false} /> : null;
          })}
        </div>
      </ScrollArea>
    </div>
  );
}
