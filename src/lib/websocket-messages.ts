import type { WebSocketMessageKind } from '@/lib/tauri-api';
import type { WebSocketDraft, WebSocketDraftMessage } from '@/types/pane-types';

export const MESSAGE_KINDS: { label: string; value: WebSocketMessageKind }[] = [
  { label: 'Text', value: 'text' },
  { label: 'JSON', value: 'json' },
  { label: 'XML', value: 'xml' },
  { label: 'Binary (base64)', value: 'binary' },
];

export function newMessage(title: string): WebSocketDraftMessage {
  return { id: crypto.randomUUID(), title, selected: false, kind: 'text', data: '' };
}

/** The draft of a new WebSocket request: one selected message, settings inherited. */
export function createDefaultWebSocketDraft(): WebSocketDraft {
  return {
    messages: [{ ...newMessage('message 1'), selected: true }],
    timeoutMs: 'inherit',
    keepAliveMs: 'inherit',
    passthrough: {},
  };
}

/** Keeps exactly one message selected: the first selected one, else the first message. */
export function normalizeSelection(messages: WebSocketDraftMessage[]): WebSocketDraftMessage[] {
  if (messages.length === 0) return messages;
  const firstSelected = messages.findIndex((m) => m.selected);
  const keep = firstSelected === -1 ? 0 : firstSelected;
  return messages.map((m, index) =>
    m.selected === (index === keep) ? m : { ...m, selected: index === keep },
  );
}

/** The message Send uses: the selected one, else the first. */
export function selectedMessage(
  messages: WebSocketDraftMessage[],
): WebSocketDraftMessage | undefined {
  return messages.find((m) => m.selected) ?? messages[0];
}

export function addMessage(messages: WebSocketDraftMessage[]): WebSocketDraftMessage[] {
  const created = { ...newMessage(`message ${messages.length + 1}`), selected: true };
  return [...messages.map((m) => (m.selected ? { ...m, selected: false } : m)), created];
}

export function selectMessage(
  messages: WebSocketDraftMessage[],
  id: string,
): WebSocketDraftMessage[] {
  return normalizeSelection(
    messages.map((m) => (m.selected === (m.id === id) ? m : { ...m, selected: m.id === id })),
  );
}

export function updateMessage(
  messages: WebSocketDraftMessage[],
  id: string,
  patch: Partial<Omit<WebSocketDraftMessage, 'id'>>,
): WebSocketDraftMessage[] {
  return messages.map((m) => (m.id === id ? { ...m, ...patch, id: m.id } : m));
}

export function removeMessage(
  messages: WebSocketDraftMessage[],
  id: string,
): WebSocketDraftMessage[] {
  return normalizeSelection(messages.filter((m) => m.id !== id));
}
