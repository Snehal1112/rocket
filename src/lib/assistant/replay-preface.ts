import type { ConversationRecord, StoredMessage } from '@/lib/tauri-api';

export interface ReplayLimits {
  /** Longest preface, in characters, header and frame included. */
  maxChars: number;
}

export const DEFAULT_REPLAY_LIMITS: ReplayLimits = { maxChars: 12_000 };

export const REPLAY_PREFACE_HEADER =
  'This is a previous conversation with the user, for context only. Do not repeat its actions.';

/** The marker line that tells the agent how many older messages are missing. */
export function replayDroppedMarker(count: number): string {
  return `[${count} earlier ${count === 1 ? 'message' : 'messages'} left out]`;
}

export const REPLAY_OPEN = '<previous-conversation>';

export const REPLAY_CLOSE = '</previous-conversation>';

/** Longest body of one message before it is cut. */
export const PER_MESSAGE_MAX_CHARS = 3_000;

const CUT_MARKER = ' […cut]';

function isHighSurrogate(code: number): boolean {
  return code >= 0xd800 && code <= 0xdbff;
}

function isLowSurrogate(code: number): boolean {
  return code >= 0xdc00 && code <= 0xdfff;
}

/**
 * Keeps at most `max` UTF-16 units from the start (or the end) of `text`.
 * Backs off one unit when the cut would split a surrogate pair.
 */
export function cutSafely(text: string, max: number, from: 'start' | 'end' = 'start'): string {
  if (max <= 0) return '';
  if (text.length <= max) return text;
  if (from === 'start') {
    const cut = text.slice(0, max);
    return isHighSurrogate(cut.charCodeAt(cut.length - 1)) ? cut.slice(0, -1) : cut;
  }
  const cut = text.slice(text.length - max);
  return isLowSurrogate(cut.charCodeAt(0)) ? cut.slice(1) : cut;
}

// Every character a tokenizer could treat as a line break.
const LINE_BREAKS = /\r\n|[\r\v\f\u0085\u2028\u2029]/g;

// NUL and the C0 controls other than tab and newline, plus DEL.
// biome-ignore lint/suspicious/noControlCharactersInRegex: stripping controls is the point.
const CONTROLS = /[\u0000-\u0008\u000e-\u001f\u007f]/g;

/** Stored text with one kind of line break and no control characters. */
function clean(text: string): string {
  return text.replace(LINE_BREAKS, '\n').replace(CONTROLS, '');
}

function oneLine(text: string): string {
  return clean(text).replace(/\s+/g, ' ').trim();
}

/**
 * Stored text is quoted history. Escape it so it can never hold the frame
 * tags, and indent continuation lines so no stored line starts at column 0.
 */
function quote(text: string): string {
  return clean(text).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/\n/g, '\n  ');
}

// A cut that lands inside "&amp;" or "&lt;" leaves one of these at the end.
const PARTIAL_ENTITY = /&(?:a(?:mp?)?|l(?:t)?)?$/;

/** Caps already quoted text, so the cap counts what is really sent. */
function capQuoted(quoted: string): string {
  if (quoted.length <= PER_MESSAGE_MAX_CHARS) return quoted;
  const cut = cutSafely(quoted, PER_MESSAGE_MAX_CHARS).replace(PARTIAL_ENTITY, '');
  return `${cut}${CUT_MARKER}`;
}

interface Entry {
  label: string;
  /** Rendered, quoted body that follows the label. */
  body: string;
}

function entryOf(message: StoredMessage): Entry | undefined {
  switch (message.kind) {
    case 'user':
      return { label: 'User', body: capQuoted(quote(message.text)) };
    case 'agent':
      return { label: 'Assistant', body: capQuoted(quote(message.text)) };
    case 'tool':
      return {
        label: 'Tool call',
        body: `${capQuoted(quote(oneLine(message.title)))} (${quote(String(message.status))})`,
      };
    case 'proposal':
      return {
        label: 'Proposed change',
        body: `${capQuoted(quote(oneLine(message.summary)))} (${quote(String(message.status))})`,
      };
    case 'notice':
      // Notices are app chatter, not conversation, so they are left out on purpose.
      return undefined;
  }
}

function render(entry: Entry): string {
  return `${entry.label}: ${entry.body}`;
}

/**
 * One stored message as a preface line. Tools and proposals become one-line
 * summaries built from their title or summary and status only, so a tool
 * result can never appear. Notices are left out.
 */
export function replayLine(message: StoredMessage): string | undefined {
  const entry = entryOf(message);
  return entry ? render(entry) : undefined;
}

function frame(lines: string[], dropped: number): string {
  return [
    REPLAY_PREFACE_HEADER,
    REPLAY_OPEN,
    ...(dropped > 0 ? [replayDroppedMarker(dropped)] : []),
    ...lines,
    REPLAY_CLOSE,
  ].join('\n');
}

/**
 * The context block put in front of the first prompt of a continued
 * conversation. Each message body is capped first, then the newest messages
 * that fit in `limits.maxChars` are kept (the whole block counts), oldest
 * first, with a marker counting the older ones left out. A newest message
 * that alone is too long keeps its label and its end.
 *
 * Unreachable edge: when `maxChars` is below the length of the empty frame
 * (header and tags), the empty frame is returned anyway and exceeds it.
 */
export function buildReplayPreface(
  record: ConversationRecord,
  limits: ReplayLimits = DEFAULT_REPLAY_LIMITS,
): string {
  const entries = record.messages
    .map(entryOf)
    .filter((entry): entry is Entry => entry !== undefined);
  const lines = entries.map(render);
  const kept: string[] = [];
  for (let i = lines.length - 1; i >= 0; i -= 1) {
    if (frame([lines[i], ...kept], i).length > limits.maxChars) break;
    kept.unshift(lines[i]);
  }
  if (kept.length === 0 && entries.length > 0) {
    const newest = entries[entries.length - 1];
    const prefix = `${newest.label}: …`;
    const room = limits.maxChars - frame([prefix], entries.length - 1).length;
    if (room > 0) {
      kept.push(`${prefix}${cutSafely(newest.body, room, 'end')}`);
    }
  }
  const out = frame(kept, entries.length - kept.length);
  // The marker can push a tiny limit over, so fall back to no marker.
  return out.length > limits.maxChars ? frame(kept, 0) : out;
}

/** The composer's note about an attached preface. */
export function replaySizeHint(chars: number): string {
  return `Earlier conversation attached as context: ${chars.toLocaleString('en-US')} characters. It goes out with your first message.`;
}
