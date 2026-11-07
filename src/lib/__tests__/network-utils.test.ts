import { describe, expect, it } from 'vitest';
import {
  formatBytes,
  formatPairingCode,
  peerName,
  shortDeviceId,
  transferResultToastId,
  transferWaitingToastId,
} from '@/lib/network-utils';

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

describe('formatBytes', () => {
  it.each([
    [0, '0 bytes'],
    [1, '1 byte'],
    [1023, '1023 bytes'],
    [1024, '1 KiB'],
    [1536, '1.5 KiB'],
    [2048, '2 KiB'],
    // Rounds up to the next unit instead of showing 1024 KiB.
    [1048575, '1 MiB'],
    [1048576, '1 MiB'],
    [10 * 1024 * 1024, '10 MiB'],
    [10 * 1024 * 1024 + 1, '10 MiB'],
    // There is no GiB unit, so a huge size stays in MiB.
    [5 * 1024 * 1024 * 1024, '5120 MiB'],
  ])('formats %d bytes as %s', (bytes, text) => {
    expect(formatBytes(bytes)).toBe(text);
  });

  it.each([Number.NaN, -1, Number.POSITIVE_INFINITY])('does not guess for %s', (bytes) => {
    expect(formatBytes(bytes)).toBe('unknown size');
  });
});

describe('peerName', () => {
  it('returns the display name as given', () => {
    expect(peerName('Dana Roy')).toBe('Dana Roy');
  });

  it('falls back for an empty or blank name', () => {
    expect(peerName('')).toBe('Unnamed node');
    expect(peerName('   ')).toBe('Unnamed node');
  });
});

describe('transferResultToastId', () => {
  it('is one fixed id per transfer and differs from the offer toast', () => {
    expect(transferResultToastId('t1')).toBe('network-transfer-result-t1');
    expect(transferResultToastId('t1')).toBe(transferResultToastId('t1'));
    expect(transferResultToastId('t1')).not.toBe(transferResultToastId('t2'));
  });
});

describe('transferWaitingToastId', () => {
  it('is one fixed id per transfer and differs from the result toast', () => {
    expect(transferWaitingToastId('t1')).toBe('network-transfer-waiting-t1');
    expect(transferWaitingToastId('t1')).not.toBe(transferResultToastId('t1'));
    expect(transferWaitingToastId('t1')).not.toBe(transferWaitingToastId('t2'));
  });
});
