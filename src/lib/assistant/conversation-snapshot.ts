import { buildTimeline } from '@/lib/assistant/timeline';
import type {
  AgentProposal,
  AssistantMode,
  ConversationInput,
  StoredMessage,
} from '@/lib/tauri-api';
import type { AssistantMessage } from '@/stores/assistant-store';

/** Everything one save needs, taken from the store and the saver. */
export interface ConversationSnapshot {
  conversationId: string;
  agentConfigId: string;
  mode: AssistantMode;
  workspaceId: string;
  collections: string[];
  messages: AssistantMessage[];
  proposals: AgentProposal[];
  proposalAnchors: Record<string, string | null>;
  continuedFrom?: string;
}

/**
 * The chat as stored messages, in reading order. Proposals sit after their
 * anchor message. Streaming and empty reply segments are left out, and a
 * reply's error becomes a notice.
 */
export function toStoredMessages(
  messages: AssistantMessage[],
  proposals: AgentProposal[],
  anchors: Record<string, string | null>,
): StoredMessage[] {
  const out: StoredMessage[] = [];
  for (const entry of buildTimeline(messages, proposals, anchors)) {
    if (entry.type === 'proposal') {
      out.push({
        kind: 'proposal',
        summary: entry.proposal.summary,
        status: entry.proposal.status,
      });
      continue;
    }
    const message = entry.message;
    switch (message.kind) {
      case 'user':
        out.push({ kind: 'user', text: message.text });
        break;
      case 'agent':
        if (message.streaming) break;
        if (message.text) out.push({ kind: 'agent', text: message.text });
        if (message.error) out.push({ kind: 'notice', text: message.error });
        break;
      case 'tool':
        out.push({ kind: 'tool', title: message.title, status: message.status });
        break;
      case 'notice':
        out.push({ kind: 'notice', text: message.text });
        break;
    }
  }
  return out;
}

/** True when the agent has replied with some text. */
export function hasAgentReply(messages: StoredMessage[]): boolean {
  return messages.some((m) => m.kind === 'agent' && m.text.trim() !== '');
}

/** The save payload, or null while there is no agent reply to keep. */
export function buildConversationInput(snapshot: ConversationSnapshot): ConversationInput | null {
  const messages = toStoredMessages(
    snapshot.messages,
    snapshot.proposals,
    snapshot.proposalAnchors,
  );
  if (!hasAgentReply(messages)) return null;
  return {
    id: snapshot.conversationId,
    agentConfigId: snapshot.agentConfigId,
    mode: snapshot.mode,
    workspaceId: snapshot.workspaceId,
    collections: snapshot.collections,
    messages,
    ...(snapshot.continuedFrom ? { continuedFrom: snapshot.continuedFrom } : {}),
  };
}
