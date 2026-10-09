import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { loadRememberedModel, rememberModel } from '@/lib/assistant/model-memory';

beforeEach(() => {
  localStorage.clear();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('model memory', () => {
  it('remembers one model per agent config', () => {
    rememberModel('a1', 'opus');
    rememberModel('a2', 'sonnet');
    expect(loadRememberedModel('a1')).toBe('opus');
    expect(loadRememberedModel('a2')).toBe('sonnet');
    expect(loadRememberedModel('a3')).toBeUndefined();
  });

  it('survives storage that throws', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('denied');
    });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('denied');
    });
    expect(() => rememberModel('a1', 'opus')).not.toThrow();
    expect(loadRememberedModel('a1')).toBeUndefined();
  });
});
