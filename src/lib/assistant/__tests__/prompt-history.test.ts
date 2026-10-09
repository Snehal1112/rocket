import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  appendPromptHistory,
  clearPromptHistory,
  loadPromptHistory,
  PROMPT_HISTORY_ENTRY_MAX,
  PROMPT_HISTORY_LIMIT,
  savePromptHistory,
} from '@/lib/assistant/prompt-history';

beforeEach(() => {
  localStorage.clear();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('appendPromptHistory', () => {
  it('trims and skips blank prompts', () => {
    expect(appendPromptHistory([], '  hi  ')).toEqual(['hi']);
    expect(appendPromptHistory(['a'], '   ')).toEqual(['a']);
  });

  it('moves a repeated prompt to the newest place', () => {
    expect(appendPromptHistory(['a', 'b', 'c'], 'a')).toEqual(['b', 'c', 'a']);
  });

  it('keeps the last 50 prompts', () => {
    const full = Array.from({ length: PROMPT_HISTORY_LIMIT }, (_, i) => `p${i}`);
    const next = appendPromptHistory(full, 'new');
    expect(next).toHaveLength(PROMPT_HISTORY_LIMIT);
    expect(next[0]).toBe('p1');
    expect(next[next.length - 1]).toBe('new');
  });
});

describe('entry size', () => {
  it('cuts a long prompt when it is appended', () => {
    const next = appendPromptHistory([], 'x'.repeat(PROMPT_HISTORY_ENTRY_MAX + 500));
    expect(next[0]).toHaveLength(PROMPT_HISTORY_ENTRY_MAX);
  });

  it('cuts a long stored entry when it is loaded', () => {
    localStorage.setItem(
      'rocket-api:assistant-prompt-history:w1',
      JSON.stringify(['y'.repeat(PROMPT_HISTORY_ENTRY_MAX + 10)]),
    );
    expect(loadPromptHistory('w1')[0]).toHaveLength(PROMPT_HISTORY_ENTRY_MAX);
  });
});

describe('clearPromptHistory', () => {
  it('forgets one workspace only', () => {
    savePromptHistory('w1', ['one']);
    savePromptHistory('w2', ['two']);
    clearPromptHistory('w1');
    expect(loadPromptHistory('w1')).toEqual([]);
    expect(loadPromptHistory('w2')).toEqual(['two']);
  });

  it('survives storage that throws', () => {
    vi.spyOn(Storage.prototype, 'removeItem').mockImplementation(() => {
      throw new Error('denied');
    });
    expect(() => clearPromptHistory('w1')).not.toThrow();
  });
});

describe('load and save', () => {
  it('keeps one history per workspace', () => {
    savePromptHistory('w1', ['one']);
    savePromptHistory('w2', ['two']);
    expect(loadPromptHistory('w1')).toEqual(['one']);
    expect(loadPromptHistory('w2')).toEqual(['two']);
  });

  it('does nothing without a workspace id', () => {
    savePromptHistory('', ['x']);
    expect(localStorage.length).toBe(0);
    expect(loadPromptHistory('')).toEqual([]);
  });

  it('ignores junk', () => {
    localStorage.setItem('rocket-api:assistant-prompt-history:w1', '{not json');
    expect(loadPromptHistory('w1')).toEqual([]);
    localStorage.setItem('rocket-api:assistant-prompt-history:w1', '{"a":1}');
    expect(loadPromptHistory('w1')).toEqual([]);
    localStorage.setItem('rocket-api:assistant-prompt-history:w1', '["ok", 3, null, "fine"]');
    expect(loadPromptHistory('w1')).toEqual(['ok', 'fine']);
  });

  it('survives storage that throws', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('denied');
    });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('denied');
    });
    expect(loadPromptHistory('w1')).toEqual([]);
    expect(() => savePromptHistory('w1', ['x'])).not.toThrow();
  });
});
