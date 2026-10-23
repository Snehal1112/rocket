import { describe, expect, it } from 'vitest';
import type { RunnerTab } from '@/types/pane-types';
import { runnerScopeLabel } from './runner-scope';

function tab(over: Partial<RunnerTab>): RunnerTab {
  return {
    id: 'r',
    title: 'Run',
    isDirty: false,
    tabType: 'runner',
    collectionName: null,
    runState: 'idle',
    requests: [],
    ...over,
  };
}

describe('runnerScopeLabel', () => {
  it('names the folder when the runner is scoped to one', () => {
    expect(runnerScopeLabel(tab({ collectionName: 'demo', folderPath: 'auth/nested' }))).toBe(
      'The folder "auth/nested"',
    );
  });

  it('names the collection when there is no folder', () => {
    expect(runnerScopeLabel(tab({ collectionName: 'demo' }))).toBe('The collection "demo"');
  });

  it('falls back to a neutral label with no scope', () => {
    expect(runnerScopeLabel(tab({}))).toBe('This selection');
  });
});
