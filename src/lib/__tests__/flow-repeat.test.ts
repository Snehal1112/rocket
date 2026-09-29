import { describe, expect, it } from 'vitest';
import { DEFAULT_REPEAT_UNTIL, msToSecondsLabel } from '../flow-repeat';

describe('flow-repeat', () => {
  it('uses the spec defaults', () => {
    expect(DEFAULT_REPEAT_UNTIL).toEqual({
      condition: 'response.status === 200',
      intervalMs: 2000,
      maxAttempts: 30,
      timeoutMs: 60000,
    });
  });

  it('labels whole and fractional seconds', () => {
    expect(msToSecondsLabel(2000)).toBe('2s');
    expect(msToSecondsLabel(1500)).toBe('1.5s');
    expect(msToSecondsLabel(14230)).toBe('14.2s');
  });
});
