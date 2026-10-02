import { beforeEach, describe, expect, it } from 'vitest';
import { MAX_LOG_ENTRIES, normalizeLogLevel, useConsoleStore } from '../console-store';

beforeEach(() => {
  useConsoleStore.setState({ entries: [], logEntries: [] });
});

describe('console-store batch entry ordering', () => {
  it('addScriptEntries preserves chronological order (earliest call first)', () => {
    useConsoleStore.getState().addScriptEntries([
      { level: 'log', message: 'A', requestName: 'Get User' },
      { level: 'log', message: 'B', requestName: 'Get User' },
      { level: 'log', message: 'C', requestName: 'Get User' },
    ]);

    const messages = useConsoleStore
      .getState()
      .entries.filter((e) => e.kind === 'script')
      .map((e) => e.message);
    expect(messages).toEqual(['A', 'B', 'C']);
  });

  it('addTestEntries preserves the order test() calls ran in', () => {
    useConsoleStore.getState().addTestEntries([
      { name: 'first test', status: 'passed', error: null, requestName: 'Get User' },
      { name: 'second test', status: 'failed', error: 'boom', requestName: 'Get User' },
    ]);

    const names = useConsoleStore
      .getState()
      .entries.filter((e) => e.kind === 'test')
      .map((e) => e.name);
    expect(names).toEqual(['first test', 'second test']);
  });

  it('addScriptEntries is a no-op for an empty array', () => {
    useConsoleStore.getState().addScriptEntries([]);
    expect(useConsoleStore.getState().entries).toHaveLength(0);
  });
});

const payload = (message: string, level = 'WARN') => ({
  timestamp: '2026-10-02T10:00:00Z',
  level,
  target: 'rocket_http::client',
  message,
  fields: { status: '500' },
  spanFields: { method: 'GET' },
});

describe('console-store backend logs', () => {
  it('addLogEntry stores newest first with a normalised level', () => {
    useConsoleStore.getState().addLogEntry(payload('first', 'warn'));
    useConsoleStore.getState().addLogEntry(payload('second', 'ERROR'));
    const logs = useConsoleStore.getState().logEntries;
    expect(logs.map((l) => l.message)).toEqual(['second', 'first']);
    expect(logs.map((l) => l.level)).toEqual(['ERROR', 'WARN']);
    expect(logs[0].kind).toBe('log');
    expect(logs[0].fields).toEqual({ status: '500' });
  });

  it('normalizeLogLevel falls back to INFO for unknown values', () => {
    expect(normalizeLogLevel('DEBUG')).toBe('INFO');
    expect(normalizeLogLevel(' Warn ')).toBe('WARN');
  });

  it('caps the log buffer without touching other entries', () => {
    useConsoleStore.getState().addScriptEntry({ level: 'log', message: 'keep', requestName: 'r' });
    for (let i = 0; i < MAX_LOG_ENTRIES + 20; i++) {
      useConsoleStore.getState().addLogEntry(payload(`m${i}`));
    }
    const state = useConsoleStore.getState();
    expect(state.logEntries).toHaveLength(MAX_LOG_ENTRIES);
    expect(state.logEntries[0].message).toBe(`m${MAX_LOG_ENTRIES + 19}`);
    expect(state.entries).toHaveLength(1);
  });

  it('clearEntries clears both buffers', () => {
    useConsoleStore.getState().addScriptEntry({ level: 'log', message: 'x', requestName: 'r' });
    useConsoleStore.getState().addLogEntry(payload('y'));
    useConsoleStore.getState().clearEntries();
    expect(useConsoleStore.getState().entries).toEqual([]);
    expect(useConsoleStore.getState().logEntries).toEqual([]);
  });
});
