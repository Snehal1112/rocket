import { cn } from '@/lib/utils';
import type { ContractCounts } from '@/types/contracts';

interface StatCardProps {
  label: string;
  value: number;
  trend: string;
  warning?: boolean;
  danger?: boolean;
}

function StatCard({ label, value, trend, warning, danger }: StatCardProps) {
  return (
    <div
      className={cn(
        'flex-1 bg-card border border-border rounded-[calc(var(--radius)-2px)] p-3 flex flex-col gap-0.5',
        warning && 'border-[hsl(var(--warning)/0.4)]',
        danger && 'border-[hsl(var(--destructive)/0.4)]',
      )}
    >
      <span className='text-[11px] font-medium text-muted-foreground uppercase tracking-[0.04em]'>
        {label}
      </span>
      <span
        className={cn(
          'text-[22px] font-semibold tabular-nums tracking-tight',
          warning && 'text-[hsl(var(--warning))]',
          danger && 'text-[hsl(var(--destructive))]',
        )}
      >
        {value}
      </span>
      <span className='text-[11px] text-muted-foreground'>{trend}</span>
    </div>
  );
}

export function ContractsSummaryRow({ counts }: { counts: ContractCounts }) {
  const healthPct = counts.total > 0 ? Math.round((counts.active / counts.total) * 100) : 0;

  // Breakdown for "Changes · 30d" trend: +{add} · −{rem} · ~{mod}
  const changeTrend =
    counts.totalChanges > 0
      ? `+${counts.changesAdded} · −${counts.changesRemoved} · ~${counts.changesModified}`
      : 'No changes';

  return (
    <div className='flex gap-2 px-6 pt-[14px] pb-3 flex-shrink-0'>
      <StatCard label='Total' value={counts.total} trend={`${counts.draft} draft`} />
      <StatCard
        label='Active & healthy'
        value={counts.active}
        trend={`${healthPct}% of contracts`}
      />
      <StatCard
        label='Drifting'
        value={counts.drift}
        trend='Needs review'
        warning={counts.drift > 0}
      />
      <StatCard
        label='Breaching'
        value={counts.breach}
        trend='Consumer at risk'
        danger={counts.breach > 0}
      />
      <StatCard label='Changes · 30d' value={counts.totalChanges} trend={changeTrend} />
    </div>
  );
}
