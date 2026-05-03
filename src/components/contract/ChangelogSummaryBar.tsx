import type { ContractChangelog } from '@/lib/tauri-api';

interface ChangelogSummaryBarProps {
  changelog: ContractChangelog;
}

export function ChangelogSummaryBar({ changelog }: ChangelogSummaryBarProps) {
  const total = changelog.entries.length;
  const removed = changelog.entries.filter((e) => e.changeType === 'removed').length;
  const added = changelog.entries.filter((e) => e.changeType === 'added').length;
  const changed = changelog.entries.filter((e) => e.changeType === 'changed').length;

  const metrics = [
    {
      label: 'Total',
      value: total,
      valueClass: 'text-foreground',
      accent: 'bg-secondary',
    },
    {
      label: 'Removed',
      value: removed,
      valueClass: 'text-destructive',
      accent: 'bg-destructive/8',
    },
    {
      label: 'Added',
      value: added,
      valueClass: 'text-green-600 dark:text-green-400',
      accent: 'bg-green-500/8',
    },
    {
      label: 'Changed',
      value: changed,
      valueClass: 'text-primary',
      accent: 'bg-primary/8',
    },
  ];

  return (
    <div className='grid grid-cols-4 gap-2.5 mb-5'>
      {metrics.map((m) => (
        <div key={m.label} className={`${m.accent} rounded-lg px-3 py-2.5 border border-border/50`}>
          <p className='text-[11px] text-muted-foreground mb-1 font-medium uppercase tracking-wide'>
            {m.label}
          </p>
          <p className={`text-xl font-semibold tabular-nums ${m.valueClass}`}>{m.value}</p>
        </div>
      ))}
    </div>
  );
}
