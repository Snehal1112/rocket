import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  loadRememberedEffort,
  loadRememberedModel,
  rememberEffort,
  rememberModel,
} from '@/lib/assistant/model-memory';

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

  it('remembers one effort per agent config, apart from the model', () => {
    rememberModel('a1', 'opus');
    rememberEffort('a1', 'max');
    rememberEffort('a2', 'low');
    expect(loadRememberedEffort('a1')).toBe('max');
    expect(loadRememberedEffort('a2')).toBe('low');
    expect(loadRememberedEffort('a3')).toBeUndefined();
    expect(loadRememberedModel('a1')).toBe('opus');
  });

  it('survives storage that throws for the effort too', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('denied');
    });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('denied');
    });
    expect(() => rememberEffort('a1', 'high')).not.toThrow();
    expect(loadRememberedEffort('a1')).toBeUndefined();
  });
});
