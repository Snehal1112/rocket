import type {
  ConnectOperationRow,
  ConnectPreview,
  ConnectRequestRow,
  ConnectRow,
} from '@/lib/tauri-api';

export type ConnectSection = 'ambiguous' | 'matched' | 'suggested' | 'inSpecOnly' | 'yoursOnly';

/** `keep_skip` and `keep_add` answer a whole ambiguous group: keep every request of it. */
export type ConnectChoice = 'link' | 'add' | 'skip' | 'keep' | 'keep_skip' | 'keep_add';

export interface ConnectRowChoice {
  id: string;
  section: ConnectSection;
  /** The request of a pair, or the request of a Yours only row. */
  request?: ConnectRequestRow;
  /** The operation of a pair, or the operation of an In spec only row. */
  operation?: ConnectOperationRow;
  /** The members of an ambiguous group. */
  requests?: ConnectRequestRow[];
  operations?: ConnectOperationRow[];
  /** Null until the user picks. Only ambiguous rows start there. */
  choice: ConnectChoice | null;
  options: ConnectChoice[];
}

/** The review rows with the safe default of each section. */
export function defaultRows(preview: ConnectPreview): ConnectRowChoice[] {
  const rows: ConnectRowChoice[] = [];
  preview.ambiguous.forEach((group, i) => {
    rows.push({
      id: `ambiguous:${i}`,
      section: 'ambiguous',
      requests: group.requests,
      operations: group.operations,
      choice: null,
      options: ['keep_skip', 'keep_add'],
    });
  });
  for (const pair of preview.matched) {
    rows.push({
      id: `matched:${pair.request.uid}`,
      section: 'matched',
      request: pair.request,
      operation: pair.operation,
      choice: 'link',
      options: ['link', 'keep'],
    });
  }
  // A suggestion is only a guess from the name, so it never starts as a link.
  for (const pair of preview.suggested) {
    rows.push({
      id: `suggested:${pair.request.uid}`,
      section: 'suggested',
      request: pair.request,
      operation: pair.operation,
      choice: 'keep',
      options: ['link', 'keep'],
    });
  }
  for (const operation of preview.inSpecOnly) {
    rows.push({
      id: `inSpecOnly:${operation.key}`,
      section: 'inSpecOnly',
      operation,
      choice: 'add',
      options: ['add', 'skip'],
    });
  }
  for (const request of preview.yoursOnly) {
    rows.push({
      id: `yoursOnly:${request.uid}`,
      section: 'yoursOnly',
      request,
      choice: 'keep',
      options: ['keep'],
    });
  }
  return rows;
}

export function hasUnresolvedAmbiguity(rows: ConnectRowChoice[]): boolean {
  return rows.some((r) => r.section === 'ambiguous' && r.choice === null);
}

/** The wire rows of the choices. A pair that is not linked keeps the request and skips the operation. */
export function rowsToRequest(rows: ConnectRowChoice[]): ConnectRow[] {
  const out: ConnectRow[] = [];
  for (const row of rows) {
    const { request, operation, choice } = row;
    if (row.section === 'ambiguous') {
      if (choice === null) continue;
      for (const r of row.requests ?? []) out.push({ kind: 'keep', uid: r.uid });
      const kind = choice === 'keep_add' ? 'add' : 'skip';
      for (const o of row.operations ?? []) out.push({ kind, key: o.key });
    } else if (request && operation) {
      if (choice === 'link') out.push({ kind: 'link', uid: request.uid, key: operation.key });
      else {
        out.push({ kind: 'keep', uid: request.uid });
        out.push({ kind: 'skip', key: operation.key });
      }
    } else if (operation) {
      out.push({ kind: choice === 'add' ? 'add' : 'skip', key: operation.key });
    } else if (request) {
      out.push({ kind: 'keep', uid: request.uid });
    }
  }
  return out;
}

export interface ConnectSummary {
  linked: number;
  added: number;
  skipped: number;
}

export function summariseRows(rows: ConnectRowChoice[]): ConnectSummary {
  const summary = { linked: 0, added: 0, skipped: 0 };
  for (const r of rowsToRequest(rows)) {
    if (r.kind === 'link') summary.linked += 1;
    else if (r.kind === 'add') summary.added += 1;
    else if (r.kind === 'skip') summary.skipped += 1;
  }
  return summary;
}
