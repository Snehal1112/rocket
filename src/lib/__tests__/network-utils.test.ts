import { describe, expect, it } from 'vitest';
import { formatPairingCode, shortDeviceId } from '@/lib/network-utils';

describe('formatPairingCode', () => {
  it('splits six digits into two groups of three', () => {
    expect(formatPairingCode('482913')).toBe('482 913');
  });

  it('keeps leading zeros', () => {
    expect(formatPairingCode('048213')).toBe('048 213');
  });

  it('returns anything that is not six digits unchanged', () => {
    expect(formatPairingCode('48291')).toBe('48291');
    expect(formatPairingCode('abc')).toBe('abc');
  });
});

describe('shortDeviceId', () => {
  it('cuts a full base32 id to the short form', () => {
    expect(shortDeviceId('K7QF2M9XPYE7HWE7ABCDEFGHIJKLMNOPQRSTUVWXYZ23')).toBe('K7QF-2M9X');
  });

  it('returns a short input unchanged', () => {
    expect(shortDeviceId('AAAA-1111')).toBe('AAAA-1111');
  });
});
