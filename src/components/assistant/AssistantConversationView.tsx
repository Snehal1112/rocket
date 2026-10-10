import { ArrowLeft } from 'lucide-react';
import { useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  CONTINUE_WORKSPACE_MISMATCH,
  canContinueHere,
  continueConversation,
} from '@/lib/assistant/assistant-session';
import { useAgentConfigs } from '@/lib/queries/agent-config-queries';
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

export const CONTINUE_HINT =
  'Continue starts a new session in Ask mode with this conversation as context. The stored conversation is not changed.';

export const MISSING_AGENT_TEXT =
  'The agent of this conversation is no longer configured. Pick an agent to continue.';

/** Agent picker and Continue. Preselects the stored agent when it still exists. */
function ContinueConversation({
  record,
  onContinued,
  onContinueFailed,
}: {
  record: ConversationRecord;
  onContinued: () => void;
  onContinueFailed?: () => void;
}) {
  const { data: agentConfigs = [] } = useAgentConfigs();
  const storedAgentExists = agentConfigs.some((c) => c.id === record.agentConfigId);
  const [picked, setPicked] = useState('');
  const agentConfigId = picked || (storedAgentExists ? record.agentConfigId : '');

  const handleContinue = () => {
    if (!agentConfigId) return;
    // A conversation of another workspace is refused and the history stays open.
    if (!canContinueHere(record)) {
      toast.error(CONTINUE_WORKSPACE_MISMATCH);
      return;
    }
    onContinued();
    // A failed start shows its error in the Start view, like any start.
    continueConversation(record, agentConfigId).catch((err: unknown) => {
      // Only a throw before the start gets here. The message is never logged.
      const refused = err instanceof Error && err.message === CONTINUE_WORKSPACE_MISMATCH;
      toast.error(refused ? CONTINUE_WORKSPACE_MISMATCH : 'Could not continue this conversation.');
      // When the start never began, the chat has nothing new, so the history comes back.
      if (!(err instanceof Error && 'started' in err && err.started === true)) onContinueFailed?.();
    });
  };

  return (
    <div className='flex shrink-0 flex-col gap-2 border-t p-3'>
      {!storedAgentExists && <p className='text-xs text-muted-foreground'>{MISSING_AGENT_TEXT}</p>}
      <Select value={agentConfigId} onValueChange={setPicked}>
        <SelectTrigger className='h-8 text-sm' aria-label='Agent'>
          <SelectValue placeholder='Pick an agent' />
        </SelectTrigger>
        <SelectContent>
          {agentConfigs.map((c) => (
            <SelectItem key={c.id} value={c.id}>
              {c.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Button size='sm' disabled={!agentConfigId} onClick={handleContinue}>
        Continue
      </Button>
      <p className='text-xs text-muted-foreground'>{CONTINUE_HINT}</p>
    </div>
  );
}

/** One stored conversation, read-only. Proposals are history lines without actions. */
export function AssistantConversationView({
  record,
  agentLabel,
  onBack,
  onContinued,
  onContinueFailed,
}: {
  record: ConversationRecord;
  agentLabel: string;
  onBack: () => void;
  onContinued?: () => void;
  onContinueFailed?: () => void;
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
      {onContinued && (
        <ContinueConversation
          record={record}
          onContinued={onContinued}
          onContinueFailed={onContinueFailed}
        />
      )}
    </div>
  );
}
