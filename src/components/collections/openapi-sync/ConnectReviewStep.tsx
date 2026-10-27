import { Badge } from '@/components/ui/badge';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  type ConnectChoice,
  type ConnectRowChoice,
  type ConnectSection,
  summariseRows,
} from './connectRows';

const CHOICE_LABEL: Record<ConnectChoice, string> = {
  link: 'Link to spec',
  keep: 'Keep mine',
  add: 'Add to collection',
  skip: 'Skip',
  keep_skip: 'Keep mine, skip the spec ones',
  keep_add: 'Keep mine, add the spec ones',
};

const SECTIONS: Array<{ section: ConnectSection; title: string; note?: string }> = [
  {
    section: 'ambiguous',
    title: 'Needs your decision',
    note: 'Several of your requests and spec operations look alike. Pick what to do with each group.',
  },
  { section: 'matched', title: 'Matched', note: 'Same method and path. These link by default.' },
  {
    section: 'suggested',
    title: 'Suggested',
    note: 'Only a guess from the name, so these are kept as they are unless you link them.',
  },
  { section: 'inSpecOnly', title: 'In spec only', note: 'These are added to your collection.' },
  { section: 'yoursOnly', title: 'Yours only', note: 'These stay as they are.' },
];

interface ConnectReviewStepProps {
  rows: ConnectRowChoice[];
  onChange: (id: string, choice: ConnectChoice) => void;
}

function Method({ value }: { value: string }) {
  return <span className='shrink-0 font-mono text-xs font-semibold'>{value}</span>;
}

function Line({ row }: { row: ConnectRowChoice }) {
  const { request, operation } = row;
  return (
    <div className='min-w-0 flex-1 space-y-0.5'>
      {request && (
        <div className='flex items-center gap-2 text-xs'>
          <Method value={request.method} />
          <span className='truncate font-mono'>{request.url}</span>
          <span className='truncate text-muted-foreground'>{request.name}</span>
        </div>
      )}
      {operation && (
        <div className='flex items-center gap-2 text-xs'>
          {request && <Badge variant='outline'>Spec</Badge>}
          <Method value={operation.method} />
          <span className='truncate font-mono'>{operation.path}</span>
          {operation.summary && (
            <span className='truncate text-muted-foreground'>{operation.summary}</span>
          )}
        </div>
      )}
    </div>
  );
}

function Group({ row }: { row: ConnectRowChoice }) {
  return (
    <div className='min-w-0 flex-1 space-y-1'>
      {(row.requests ?? []).map((r) => (
        <div key={r.uid} className='flex items-center gap-2 text-xs'>
          <Method value={r.method} />
          <span className='truncate font-mono'>{r.url}</span>
        </div>
      ))}
      {(row.operations ?? []).map((o) => (
        <div key={o.key} className='flex items-center gap-2 text-xs'>
          <Badge variant='outline'>Spec</Badge>
          <Method value={o.method} />
          <span className='truncate font-mono'>{o.path}</span>
        </div>
      ))}
    </div>
  );
}

function rowLabel(row: ConnectRowChoice, groupNumber: number): string {
  if (row.section === 'ambiguous') return `Choice for tangled group ${groupNumber}`;
  return `Choice for ${row.operation?.key ?? `${row.request?.method} ${row.request?.url}`}`;
}

/** Step two: every request and operation with its choice, and a count of what Connect will do. */
export function ConnectReviewStep({ rows, onChange }: ConnectReviewStepProps) {
  const summary = summariseRows(rows);
  return (
    <div className='flex flex-col gap-4'>
      {SECTIONS.map(({ section, title, note }) => {
        const inSection = rows.filter((r) => r.section === section);
        if (inSection.length === 0) return null;
        return (
          <section key={section} className='space-y-2'>
            <h3 className='text-sm font-semibold'>
              {title} ({inSection.length})
            </h3>
            {note && <p className='text-xs text-muted-foreground'>{note}</p>}
            <ul className='space-y-1.5'>
              {inSection.map((row, i) => (
                <li key={row.id} className='flex items-center gap-3 rounded-md border p-2'>
                  {row.section === 'ambiguous' ? <Group row={row} /> : <Line row={row} />}
                  {row.options.length > 1 ? (
                    <Select
                      value={row.choice ?? undefined}
                      onValueChange={(v) => onChange(row.id, v as ConnectChoice)}
                    >
                      <SelectTrigger className='w-52 shrink-0' aria-label={rowLabel(row, i + 1)}>
                        <SelectValue placeholder='Pick one' />
                      </SelectTrigger>
                      <SelectContent>
                        {row.options.map((o) => (
                          <SelectItem key={o} value={o}>
                            {CHOICE_LABEL[o]}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  ) : (
                    <span className='w-52 shrink-0 text-xs text-muted-foreground'>
                      {row.choice ? CHOICE_LABEL[row.choice] : ''}
                    </span>
                  )}
                </li>
              ))}
            </ul>
          </section>
        );
      })}
      <p className='text-sm font-medium'>
        {summary.linked} linked, {summary.added} added, {summary.skipped} skipped
      </p>
    </div>
  );
}
