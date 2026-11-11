import { describe, expect, it } from 'vitest';
import { receivedFromText, receivedOnText } from '@/lib/received-from';
import type { ReceivedCollection } from '@/lib/tauri-api';

const origin = (over: Partial<ReceivedCollection> = {}): ReceivedCollection => ({
  collectionUid: 'u1',
  senderName: 'Ana Lima',
  senderShortId: '50L2-DPML',
  receivedAt: '2026-11-11T09:00:00Z',
  ...over,
});

describe('received-from texts', () => {
  it('names the sender', () => {
    expect(receivedFromText(origin())).toBe('Received from Ana Lima');
  });

  it('names a sender without a display name', () => {
    expect(receivedFromText(origin({ senderName: '  ' }))).toBe('Received from Unnamed node');
  });

  it('formats the date and survives a bad one', () => {
    expect(receivedOnText(origin())).toMatch(/2026/);
    expect(receivedOnText(origin({ receivedAt: 'not a date' }))).toBe('');
  });
});
