import { describe, expect, it } from 'vitest';
import { formatSize, formatTime, previewPayload } from '@/lib/message-log-format';

describe('message log formatting', () => {
  it('formats a timestamp as local HH:MM:SS.mmm', () => {
    const ms = new Date(2026, 9, 5, 13, 4, 5, 7).getTime();
    expect(formatTime(ms)).toBe('13:04:05.007');
  });

  it('formats sizes in B, KB and MB', () => {
    expect(formatSize(0)).toBe('0 B');
    expect(formatSize(999)).toBe('999 B');
    expect(formatSize(1536)).toBe('1.5 KB');
    expect(formatSize(2 * 1024 * 1024)).toBe('2.0 MB');
  });

  it('previews text as is and truncates long text with the full size', () => {
    expect(previewPayload({ kind: 'text', data: 'hello', size: 5 })).toBe('hello');
    const long = 'x'.repeat(50);
    expect(previewPayload({ kind: 'text', data: long, size: 50 }, 10)).toBe(
      `${'x'.repeat(10)}… (50 bytes)`,
    );
  });

  it('previews binary frames as hex', () => {
    expect(previewPayload({ kind: 'binary', data: 'AQID', size: 3 })).toBe('01 02 03');
  });

  it('truncates long binary previews and states the total size', () => {
    const bytes = new Uint8Array(100).fill(255);
    const b64 = btoa(String.fromCharCode(...bytes));
    const preview = previewPayload({ kind: 'binary', data: b64, size: 100 });
    expect(preview.endsWith('… (100 bytes)')).toBe(true);
    expect(preview.split(' ').filter((p) => p === 'ff')).toHaveLength(64);
  });

  it('falls back to the raw data when binary data is not valid base64', () => {
    expect(previewPayload({ kind: 'binary', data: '***', size: 3 })).toBe('***');
  });
});
