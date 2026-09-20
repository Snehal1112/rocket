import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { formatRelativeTime } from '@/lib/relative-time';

describe('formatRelativeTime', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-09-20T12:00:00Z'));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('returns "just now" for under a minute', () => {
    expect(formatRelativeTime(new Date('2026-09-20T11:59:30Z').toISOString())).toBe('just now');
  });

  it('returns minutes for under an hour', () => {
    expect(formatRelativeTime(new Date('2026-09-20T11:45:00Z').toISOString())).toBe('15m ago');
  });

  it('returns hours for under a day', () => {
    expect(formatRelativeTime(new Date('2026-09-20T09:00:00Z').toISOString())).toBe('3h ago');
  });

  it('returns days for under 30 days', () => {
    expect(formatRelativeTime(new Date('2026-09-15T12:00:00Z').toISOString())).toBe('5d ago');
  });

  it('returns a short calendar date for 30 days or more', () => {
    const timestamp = new Date('2026-08-01T12:00:00Z').toISOString();
    expect(formatRelativeTime(timestamp)).toBe(
      new Date(timestamp).toLocaleDateString(undefined, { month: 'short', day: 'numeric' }),
    );
  });
});
