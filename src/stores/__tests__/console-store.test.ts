import { beforeEach, describe, expect, it } from 'vitest';
import { useConsoleStore } from '../console-store';

beforeEach(() => {
  useConsoleStore.setState({ entries: [] });
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
