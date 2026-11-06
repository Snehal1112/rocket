import { cn } from '@/lib/utils';

export interface CompareEntry {
  /** Product name. */
  name: string;
  /** The measurement, already formatted with its unit. */
  display: string;
  /** Raw magnitude used for the bar length. `null` means it was never measured. */
  value: number | null;
  /** A short qualifier, e.g. the process count behind a memory figure. */
  note?: string;
}

export interface CompareRow {
  metric: string;
  /** What "good" looks like, spelled out rather than implied by an arrow. */
  goal: string;
  entries: CompareEntry[];
}

/** One metric, drawn as three proportional bars. */
export function CompareBarGroup({ row }: { row: CompareRow }) {
  const max = Math.max(...row.entries.map((entry) => entry.value ?? 0), 1);

  return (
    <div className="py-5 sm:py-6">
      <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
        <h3 className="font-heading text-sm font-semibold tracking-tight text-foreground">
          {row.metric}
        </h3>
        <p className="font-heading text-[11px] tracking-wide text-muted-foreground">{row.goal}</p>
      </div>

      <dl className="mt-4 space-y-2.5">
        {row.entries.map((entry) => {
          const isRocket = entry.name === 'Rocket';
          const width = entry.value === null ? 0 : Math.max((entry.value / max) * 100, 1.5);

          return (
            <div
              key={entry.name}
              className="grid grid-cols-[3.75rem_1fr] items-center gap-2.5 sm:grid-cols-[6rem_1fr] sm:gap-3"
            >
              <dt
                className={cn(
                  'font-heading text-xs tracking-tight',
                  isRocket ? 'font-semibold text-foreground' : 'text-muted-foreground',
                )}
              >
                {entry.name}
              </dt>
              <dd className="flex items-center gap-3">
                <div className="h-2.5 min-w-0 flex-1 overflow-hidden rounded-full bg-muted">
                  {entry.value === null ? null : (
                    <div
                      className={cn(
                        'h-full rounded-full',
                        isRocket ? 'bg-primary' : 'bg-muted-foreground/35',
                      )}
                      style={{ width: `${width}%` }}
                    />
                  )}
                </div>
                <span
                  className={cn(
                    'w-[4.75rem] shrink-0 text-right font-heading text-[11px] tabular-nums sm:w-32 sm:text-xs',
                    isRocket ? 'font-semibold text-foreground' : 'text-muted-foreground',
                  )}
                >
                  {entry.display}
                  {entry.note ? (
                    <span className="ml-1 hidden font-normal text-muted-foreground/70 sm:inline">
                      {entry.note}
                    </span>
                  ) : null}
                </span>
              </dd>
            </div>
          );
        })}
      </dl>
    </div>
  );
}
