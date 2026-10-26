import type { ImportReport, SkippedItem, SkipReason } from '@/lib/tauri-api';

/** Notes that share one cause, such as several references to the same file. */
export interface NoteGroup {
  /** The file the notes are about, or the whole note when it has no known file. */
  label: string;
  count: number;
  /** The file name is known, so the row can use the "another file" sentence. */
  isFile: boolean;
  /** Where the notes came from, for the tooltip. */
  paths: string[];
  /** The original notes, for the tooltip. */
  details: string[];
}

export interface ReportView {
  /** Items that were really not imported. */
  skipped: SkippedItem[];
  /** Things that were left out of imported requests, grouped by cause. */
  notes: NoteGroup[];
}

const NOT_LOADED = /^a \$ref to (.+) was not loaded$/;

/** A short label for a reason. The raw enum name is never shown. */
export function reasonLabel(reason: SkipReason): string {
  switch (reason.type) {
    case 'unsupportedRequestType':
      return 'Unsupported request';
    case 'unsupportedAuthType':
      return 'Unsupported auth';
    case 'parseError':
      return 'Could not read';
    case 'notFollowed':
      return 'Not loaded';
    default:
      return 'Not imported';
  }
}

/** Splits the report into really skipped items and grouped notes. */
export function buildReportView(report: Pick<ImportReport, 'skipped'>): ReportView {
  const skipped: SkippedItem[] = [];
  const groups = new Map<string, NoteGroup>();
  for (const item of report.skipped) {
    if (item.reason.type !== 'notFollowed') {
      skipped.push(item);
      continue;
    }
    const detail = item.reason.detail;
    const match = NOT_LOADED.exec(detail);
    const key = match ? `file:${match[1]}` : `text:${detail}`;
    let group = groups.get(key);
    if (!group) {
      group = {
        label: match ? match[1] : detail,
        count: 0,
        isFile: match !== null,
        paths: [],
        details: [],
      };
      groups.set(key, group);
    }
    group.count += 1;
    if (!group.paths.includes(item.path)) group.paths.push(item.path);
    if (!group.details.includes(detail)) group.details.push(detail);
  }
  return { skipped, notes: [...groups.values()] };
}

/** The sentence for one group of notes. */
export function describeNoteGroup(group: NoteGroup): string {
  if (!group.isFile) return group.label;
  const many = group.count !== 1;
  return `${group.count} reference${many ? 's' : ''} ${many ? 'come' : 'comes'} from another file that Rocket did not load: ${group.label}. The requests were imported without ${many ? 'them' : 'it'}.`;
}

/** "N skipped" for items that were really not imported, or null when there are none. */
export function skippedSummary(view: ReportView): string | null {
  const n = view.skipped.length;
  return n > 0 ? `${n} item${n !== 1 ? 's' : ''} skipped` : null;
}

/** "M notes" for things left out of imported requests, or null when there are none. */
export function notesSummary(view: ReportView): string | null {
  const n = view.notes.length;
  return n > 0 ? `${n} note${n !== 1 ? 's' : ''}` : null;
}
