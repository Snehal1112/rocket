import { describe, expect, it } from 'vitest';
import {
  conflictKindText,
  keepBothAllowed,
  linkStatusText,
  SAFE_SANDBOX_NOTE,
  syncCountsText,
  syncResultToastId,
  syncWaitingToastId,
} from '@/lib/network-sync-utils';

describe('linkStatusText', () => {
  it.each([
    ['in_sync', 'In sync with Ana Lima'],
    ['changes_waiting', 'Changes waiting to sync with Ana Lima'],
    ['out_of_date', 'Out of date with Ana Lima. Sync again.'],
    ['broken', 'Link to Ana Lima is broken. Unlink and send the collection again.'],
  ] as const)('describes %s', (state, text) => {
    expect(linkStatusText(state, 'Ana Lima')).toBe(text);
  });

  it('names a node without a display name', () => {
    expect(linkStatusText('in_sync', '  ')).toBe('In sync with Unnamed node');
  });

  it('says nothing wrong about a state added later', () => {
    expect(linkStatusText('future' as never, 'Ana Lima')).toBe('Linked with Ana Lima');
  });
});

describe('syncCountsText', () => {
  it('lists only the counts above zero', () => {
    expect(syncCountsText(2, 0, 1)).toBe('2 added, 1 deleted');
    expect(syncCountsText(0, 3, 0)).toBe('3 changed');
  });

  it('says no changes for all zero', () => {
    expect(syncCountsText(0, 0, 0)).toBe('no changes');
  });
});

describe('conflictKindText', () => {
  it.each([
    ['both_changed', 'Changed on both nodes'],
    ['delete_vs_edit', 'Deleted on one node, changed on the other'],
    ['both_added', 'Added on both nodes with different content'],
    ['rename_vs_edit', 'Renamed on one node, changed on the other'],
  ] as const)('describes %s', (kind, text) => {
    expect(conflictKindText(kind)).toBe(text);
  });

  it('has a fallback for a kind added later', () => {
    expect(conflictKindText('future' as never)).toBe('Changed on both nodes');
  });
});

describe('ids and notes', () => {
  it('gives the waiting toast and the result toast different ids', () => {
    expect(syncWaitingToastId('s1')).toBe('network-sync-waiting-s1');
    expect(syncResultToastId('s1')).toBe('network-sync-result-s1');
  });

  it('says changed scripts keep the current script access', () => {
    expect(SAFE_SANDBOX_NOTE).toBe(
      "Changed scripts run with this collection's current script access. A sync never changes trust settings.",
    );
  });
});

describe('keepBothAllowed', () => {
  it.each([
    ['opencollection.yml', false],
    ['OpenCollection.yml', false],
    ['folder.yml', false],
    ['Orders/Sub/Folder.yml', false],
    ['Orders/get-user.yml', true],
    ['sub/opencollection.yml', true],
    ['folder.yml.bak', true],
  ])('%s -> %s', (path, expected) => {
    expect(keepBothAllowed(path)).toBe(expected);
  });
});
