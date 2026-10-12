import type { AgentProposal } from '@/lib/tauri-api';
import type { AssistantMessage, ToolActivityMessage } from '@/stores/assistant-store';

export type ToolMessage = ToolActivityMessage;

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

export type GroupedEntry = TimelineEntry | { type: 'tools'; id: string; tools: ToolMessage[] };

/** Folds each run of consecutive tool messages into one entry. */
export function groupToolRuns(entries: TimelineEntry[]): GroupedEntry[] {
  const out: GroupedEntry[] = [];
  for (const entry of entries) {
    if (entry.type === 'message' && entry.message.kind === 'tool') {
      const last = out[out.length - 1];
      if (last?.type === 'tools') {
        last.tools.push(entry.message);
      } else {
        out.push({ type: 'tools', id: entry.message.id, tools: [entry.message] });
      }
    } else {
      out.push(entry);
    }
  }
  return out;
}
