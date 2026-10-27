import { describe, expect, it } from 'vitest';
import { chipState } from '../chipState';
import { makeStatus } from './fixtures';

describe('chipState', () => {
  const cases: Array<[string, Parameters<typeof makeStatus>[0], number, string, string]> = [
    ['consent missing', { consent: 'missing' }, 0, 'needs-approval', 'Needs approval'],
    [
      'consent location changed beats a failure',
      { consent: 'locationChanged', lastStatus: { state: 'failed' } },
      2,
      'needs-approval',
      'Needs approval',
    ],
    [
      'a failure beats waiting updates',
      { lastStatus: { state: 'failed' }, lastMessage: 'Timed out.' },
      3,
      'failed',
      'Check failed',
    ],
    ['updates wait', { lastStatus: { state: 'unchanged' } }, 3, 'updates', '3 updates'],
    ['one update waits', { lastStatus: { state: 'unchanged' } }, 1, 'updates', '1 update'],
    ['checks are off', { interval: 'off', lastStatus: { state: 'unchanged' } }, 0, 'off', 'Checks off'],
    [
      'connected',
      { interval: 'every_5_minutes', lastStatus: { state: 'unchanged' } },
      0,
      'connected',
      'Connected',
    ],
  ];
  it.each(cases)('%s', (_name, overrides, waiting, kind, label) => {
    const chip = chipState(makeStatus(overrides), waiting);
    expect(chip.kind).toBe(kind);
    expect(chip.label).toBe(label);
  });

  it('carries the message of a failure as detail', () => {
    const chip = chipState(
      makeStatus({ lastStatus: { state: 'failed' }, lastMessage: 'Timed out.' }),
      3,
    );
    expect(chip.detail).toBe('Timed out.');
  });
});
