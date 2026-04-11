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
      label: 'Total changes',
      value: total,
      valueClass: 'text-foreground',
    },
    {
      label: 'Removed',
      value: removed,
      valueClass: 'text-destructive',
    },
    {
      label: 'Added',
      value: added,
      valueClass: 'text-green-600 dark:text-green-400',
    },
    {
      label: 'Changed',
      value: changed,
      valueClass: 'text-blue-600 dark:text-blue-400',
    },
  ];

  return (
    <div className='grid grid-cols-4 gap-3 mb-6'>
      {metrics.map((m) => (
        <div key={m.label} className='bg-secondary rounded-lg p-3'>
          <p className='text-xs text-muted-foreground mb-1'>{m.label}</p>
          <p className={`text-2xl font-medium tabular-nums ${m.valueClass}`}>{m.value}</p>
        </div>
      ))}
    </div>
  );
}
