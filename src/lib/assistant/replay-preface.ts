import type { ConversationRecord, StoredMessage } from '@/lib/tauri-api';

export interface ReplayLimits {
  /** Longest preface, in characters, header and frame included. */
  maxChars: number;
}

export const DEFAULT_REPLAY_LIMITS: ReplayLimits = { maxChars: 12_000 };

export const REPLAY_PREFACE_HEADER =
  'This is a previous conversation with the user, for context only. Do not repeat its actions.';

export const REPLAY_DROPPED_MARKER = '[Older messages of this conversation were left out.]';

export const REPLAY_OPEN = '<previous-conversation>';

export const REPLAY_CLOSE = '</previous-conversation>';

function oneLine(text: string): string {
  return text.replace(/\s+/g, ' ').trim();
}

/**
 * One stored message as a preface line. Tools and proposals become one-line
 * summaries built from their title or summary and status only, so a tool
 * result can never appear. Notices are left out.
 */
export function replayLine(message: StoredMessage): string | undefined {
  switch (message.kind) {
    case 'user':
      return `User: ${message.text}`;
    case 'agent':
      return `Assistant: ${message.text}`;
    case 'tool':
      return `Tool call: ${oneLine(message.title)} (${message.status})`;
    case 'proposal':
      return `Proposed change: ${oneLine(message.summary)} (${message.status})`;
    case 'notice':
      return undefined;
  }
}

function frame(lines: string[], dropped: boolean): string {
  return [
    REPLAY_PREFACE_HEADER,
    REPLAY_OPEN,
    ...(dropped ? [REPLAY_DROPPED_MARKER] : []),
    ...lines,
    REPLAY_CLOSE,
  ].join('\n');
}

/**
 * The context block put in front of the first prompt of a continued
 * conversation. Keeps the newest messages that fit in `limits.maxChars`
 * (the whole block counts), oldest first, with a marker when older ones
 * were left out. A newest message that alone is too long keeps its end.
 */
export function buildReplayPreface(
  record: ConversationRecord,
  limits: ReplayLimits = DEFAULT_REPLAY_LIMITS,
): string {
  const lines = record.messages
    .map(replayLine)
    .filter((line): line is string => line !== undefined);
  const kept: string[] = [];
  for (let i = lines.length - 1; i >= 0; i -= 1) {
    if (frame([lines[i], ...kept], i > 0).length > limits.maxChars) break;
    kept.unshift(lines[i]);
  }
  if (kept.length === 0 && lines.length > 0) {
    const newest = lines[lines.length - 1];
    const room = Math.max(0, limits.maxChars - frame([''], true).length - 1);
    kept.push(`…${newest.slice(Math.max(0, newest.length - room))}`);
  }
  return frame(kept, kept.length < lines.length);
}

/** The composer's note about an attached preface. */
export function replaySizeHint(chars: number): string {
  return `Earlier conversation attached as context: ${chars.toLocaleString('en-US')} characters. It goes out with your first message.`;
}
