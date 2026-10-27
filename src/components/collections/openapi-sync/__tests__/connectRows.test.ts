import { describe, expect, it } from 'vitest';
import {
  defaultRows,
  hasUnresolvedAmbiguity,
  rowsToRequest,
  summariseRows,
} from '../connectRows';
import { makeConnectPreview } from './fixtures';

const section = (rows: ReturnType<typeof defaultRows>, name: string) =>
  rows.filter((r) => r.section === name);

describe('connectRows', () => {
  it('matched_rows_default_to_link', () => {
    const rows = defaultRows(makeConnectPreview());
    expect(section(rows, 'matched').map((r) => r.choice)).toEqual(['link']);
  });

  it('suggested_rows_default_to_keep', () => {
    const rows = defaultRows(makeConnectPreview());
    expect(section(rows, 'suggested').map((r) => r.choice)).toEqual(['keep']);
  });

  it('spec_only_rows_default_to_add', () => {
    const rows = defaultRows(makeConnectPreview());
    expect(section(rows, 'inSpecOnly').map((r) => r.choice)).toEqual(['add']);
  });

  it('yours_only_rows_default_to_keep', () => {
    const rows = defaultRows(makeConnectPreview());
    expect(section(rows, 'yoursOnly').map((r) => r.choice)).toEqual(['keep']);
  });

  it('ambiguous_rows_have_no_default_and_block_continue', () => {
    const preview = makeConnectPreview({
      ambiguous: [
        {
          requests: makeConnectPreview().yoursOnly,
          operations: makeConnectPreview().inSpecOnly,
        },
      ],
    });
    const rows = defaultRows(preview);
    const ambiguous = section(rows, 'ambiguous');
    expect(ambiguous.map((r) => r.choice)).toEqual([null]);
    expect(hasUnresolvedAmbiguity(rows)).toBe(true);
    const picked = rows.map((r) => (r.section === 'ambiguous' ? { ...r, choice: 'keep_skip' as const } : r));
    expect(hasUnresolvedAmbiguity(picked)).toBe(false);
  });

  it('rows_to_request_sends_the_choice_key_and_uid', () => {
    const rows = defaultRows(makeConnectPreview());
    expect(rowsToRequest(rows)).toEqual([
      { kind: 'link', uid: 'r1', key: 'GET /pets' },
      { kind: 'keep', uid: 'r2' },
      { kind: 'skip', key: 'GET /pets/all' },
      { kind: 'add', key: 'POST /pets' },
      { kind: 'keep', uid: 'r3' },
    ]);
  });

  it('an_ambiguous_group_keeps_its_requests_and_adds_or_skips_its_operations', () => {
    const base = makeConnectPreview();
    const rows = defaultRows(
      makeConnectPreview({
        matched: [],
        suggested: [],
        inSpecOnly: [],
        yoursOnly: [],
        ambiguous: [{ requests: base.yoursOnly, operations: base.inSpecOnly }],
      }),
    );
    const add = rows.map((r) => ({ ...r, choice: 'keep_add' as const }));
    expect(rowsToRequest(add)).toEqual([
      { kind: 'keep', uid: 'r3' },
      { kind: 'add', key: 'POST /pets' },
    ]);
    const skip = rows.map((r) => ({ ...r, choice: 'keep_skip' as const }));
    expect(rowsToRequest(skip)).toEqual([
      { kind: 'keep', uid: 'r3' },
      { kind: 'skip', key: 'POST /pets' },
    ]);
  });

  it('the_summary_counts_linked_added_and_skipped', () => {
    const rows = defaultRows(makeConnectPreview());
    expect(summariseRows(rows)).toEqual({ linked: 1, added: 1, skipped: 1 });
  });
});
