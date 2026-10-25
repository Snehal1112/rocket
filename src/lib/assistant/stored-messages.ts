import type { StoredMessage } from '@/lib/tauri-api';
import type { AssistantMessage } from '@/stores/assistant-store';

const PROPOSAL_STATUS_LABEL = {
  pending: 'not decided',
  accepted: 'accepted',
  rejected: 'rejected',
  stale: 'stale',
  failed: 'failed',
} as const;

/**
 * Stored messages as live chat messages, so a conversation can carry on in the chat.
 * A stored proposal has no live card, so it becomes a notice line with its status.
 */
export function storedToChatMessages(messages: StoredMessage[]): AssistantMessage[] {
  return messages.map(toChatMessage);
}

function toChatMessage(message: StoredMessage): AssistantMessage {
  const id = crypto.randomUUID();
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
      return {
        kind: 'notice',
        id,
        text: `Proposal: ${message.summary} (${PROPOSAL_STATUS_LABEL[message.status]})`,
      };
  }
}
