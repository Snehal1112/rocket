import { describe, expect, it } from 'vitest';
import { formatUsage } from '../usage-format';

describe('formatUsage', () => {
  it('hides the indicator without usage or with a zero size', () => {
    expect(formatUsage(undefined)).toBeNull();
    expect(formatUsage({ used: 10, size: 0 })).toBeNull();
  });

  it('rounds the percentage and lists the tokens', () => {
    expect(formatUsage({ used: 24_000, size: 200_000 })).toEqual({
      percent: 12,
      text: '12%',
      detail: '24,000 of 200,000 tokens',
    });
  });

  it('adds the cost when the agent reports it', () => {
    expect(formatUsage({ used: 1, size: 100, costUsd: 0.01234 })?.detail).toBe(
      '1 of 100 tokens, $0.0123',
    );
  });

  it('never shows more than 100%', () => {
    expect(formatUsage({ used: 300, size: 100 })?.text).toBe('100%');
  });
});
