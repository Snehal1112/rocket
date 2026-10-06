import type { MessageLogEntry } from '@/types/message-log';

const pad = (value: number, width: number) => String(value).padStart(width, '0');

/** Local time as HH:MM:SS.mmm. */
export function formatTime(timestampMs: number): string {
  const d = new Date(timestampMs);
  return `${pad(d.getHours(), 2)}:${pad(d.getMinutes(), 2)}:${pad(d.getSeconds(), 2)}.${pad(d.getMilliseconds(), 3)}`;
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

const MAX_HEX_BYTES = 64;

/** A single-string preview of an entry's payload: text, truncated, or hex for binary. */
export function previewPayload(
  entry: Pick<MessageLogEntry, 'kind' | 'data' | 'size'>,
  maxChars = 2000,
): string {
  if (entry.kind === 'text') {
    return entry.data.length > maxChars
      ? `${entry.data.slice(0, maxChars)}… (${entry.size} bytes)`
      : entry.data;
  }
  try {
    const raw = atob(entry.data);
    const hex = Array.from(raw.slice(0, MAX_HEX_BYTES), (c) =>
      c.charCodeAt(0).toString(16).padStart(2, '0'),
    ).join(' ');
    return raw.length > MAX_HEX_BYTES ? `${hex} … (${entry.size} bytes)` : hex;
  } catch {
    return entry.data;
  }
}
