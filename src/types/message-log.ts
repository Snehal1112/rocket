export type MessageDirection = 'in' | 'out' | 'system';

/**
 * One line of a live message log. Shared by the WebSocket tab and the GraphQL subscription
 * panel. `system` entries are produced by the frontend (connected, closed) and carry size 0.
 */
export interface MessageLogEntry {
  id: string;
  direction: MessageDirection;
  /** Short tag shown as a badge, such as `next` or `error` for subscription results. */
  label?: string;
  kind: 'text' | 'binary';
  /** Text as is, or base64 when `kind` is `binary`. */
  data: string;
  /** Payload size in bytes. */
  size: number;
  timestampMs: number;
}
