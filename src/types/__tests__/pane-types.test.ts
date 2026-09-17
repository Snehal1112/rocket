import { describe, expect, it } from 'vitest';
import { isRunnerTab } from '@/types/pane-types';
import type { RequestTab, RunnerTab } from '@/types/pane-types';

describe('isRunnerTab', () => {
  it('returns true for a runner tab', () => {
    const tab: RunnerTab = {
      id: 'runner-1',
      title: 'Runner',
      isDirty: false,
      tabType: 'runner',
      collectionName: 'demo',
      runState: 'idle',
      requests: [],
    };
    expect(isRunnerTab(tab)).toBe(true);
  });

  it('returns false for a non-runner tab', () => {
    const tab: RequestTab = {
      id: 'req-1',
      title: 'Request',
      isDirty: false,
      tabType: 'request',
      request: {} as RequestTab['request'],
      response: null,
    };
    expect(isRunnerTab(tab)).toBe(false);
  });
});
