import type { AgentProposal } from '@/lib/tauri-api';
import type { AssistantMessage } from '@/stores/assistant-store';

export type TimelineEntry =
  | { type: 'message'; message: AssistantMessage }
  | { type: 'proposal'; proposal: AgentProposal };

/** Interleaves messages with resolved proposals placed after their anchor. */
export function buildTimeline(
  messages: AssistantMessage[],
  resolved: AgentProposal[],
  anchors: Record<string, string | null>,
): TimelineEntry[] {
  const messageIds = new Set(messages.map((m) => m.id));
  const byAnchor = new Map<string | null, AgentProposal[]>();
  for (const p of resolved) {
    const anchor = anchors[p.id] ?? null;
    // A gone anchor message falls back to the end of the chat.
    const key = anchor === null || messageIds.has(anchor) ? anchor : '__end__';
    const list = byAnchor.get(key) ?? [];
    list.push(p);
    byAnchor.set(key, list);
  }
  const place = (key: string | null): TimelineEntry[] =>
    (byAnchor.get(key) ?? [])
      .slice()
      .sort((a, b) => a.createdAtMs - b.createdAtMs)
      .map((proposal) => ({ type: 'proposal', proposal }));
  const entries: TimelineEntry[] = place(null);
  for (const message of messages) {
    entries.push({ type: 'message', message }, ...place(message.id));
  }
  entries.push(...place('__end__'));
  return entries;
}
