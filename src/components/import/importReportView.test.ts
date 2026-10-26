import { describe, expect, it } from 'vitest';
import type { SkippedItem } from '@/lib/tauri-api';
import {
  buildReportView,
  describeNoteGroup,
  notesSummary,
  reasonLabel,
  skippedSummary,
} from './importReportView';

const note = (detail: string): SkippedItem => ({
  path: 'api-example.yaml',
  reason: { type: 'notFollowed', detail },
});
const lost = (): SkippedItem => ({
  path: 'a.yml',
  reason: { type: 'parseError', detail: 'bad' },
});

describe('import report view', () => {
  it('groups_not_followed_notes_by_cause', () => {
    const view = buildReportView({
      skipped: [
        note('a $ref to definitions.yaml was not loaded'),
        note('a $ref to definitions.yaml was not loaded'),
        note('a $ref to definitions.yaml was not loaded'),
        note('a $ref to other.yaml was not loaded'),
      ],
    });
    expect(view.skipped).toHaveLength(0);
    expect(view.notes).toHaveLength(2);
    expect(view.notes[0].count).toBe(3);
    expect(describeNoteGroup(view.notes[0])).toBe(
      '3 references come from another file that Rocket did not load: definitions.yaml. The requests were imported without them.',
    );
    expect(describeNoteGroup(view.notes[1])).toContain('1 reference comes from');
  });

  it('summary_does_not_call_notes_skipped', () => {
    const onlyNotes = buildReportView({ skipped: [note('a $ref to d.yaml was not loaded')] });
    expect(skippedSummary(onlyNotes)).toBeNull();
    expect(notesSummary(onlyNotes)).toBe('1 note');
    const mixed = buildReportView({
      skipped: [
        lost(),
        note('a $ref to d.yaml was not loaded'),
        note('a $ref to e.yaml was not loaded'),
      ],
    });
    expect(skippedSummary(mixed)).toBe('1 item skipped');
    expect(notesSummary(mixed)).toBe('2 notes');
  });

  it('never_shows_the_raw_enum_name', () => {
    expect(reasonLabel({ type: 'notFollowed', detail: '' })).toBe('Not loaded');
    for (const type of ['unsupportedRequestType', 'unsupportedAuthType', 'parseError'] as const) {
      expect(reasonLabel({ type, detail: '' })).not.toContain(type);
    }
    expect(reasonLabel({ type: 'somethingNew' as never, detail: '' })).toBe('Not imported');
  });
});
