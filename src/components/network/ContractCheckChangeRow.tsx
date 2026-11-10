import { CircleSlash, ShieldAlert, ShieldCheck } from 'lucide-react';
import { ContractCheckChangeDiff } from '@/components/network/ContractCheckChangeDiff';
import { Badge } from '@/components/ui/badge';
import { verdictLabel } from '@/lib/network-contract-check-utils';
import type { ContractVerdict, ReviewedContractChange } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';

const VERDICT_STYLE: Record<
  ContractVerdict,
  { rail: string; variant: 'success' | 'destructive' | 'outline' }
> = {
  pass: { rail: 'border-l-[hsl(var(--toast-success-fg))]', variant: 'success' },
  break: { rail: 'border-l-destructive-solid', variant: 'destructive' },
  not_used: { rail: 'border-l-border', variant: 'outline' },
};

function VerdictIcon({ verdict }: { verdict: ContractVerdict }) {
  const cls = 'h-3 w-3';
  if (verdict === 'pass') return <ShieldCheck className={cls} aria-hidden='true' />;
  if (verdict === 'break') return <ShieldAlert className={cls} aria-hidden='true' />;
  return <CircleSlash className={cls} aria-hidden='true' />;
}

/** One reviewed change. The rail on the left carries the verdict. All text is peer text or local. */
export function ContractCheckChangeRow({
  checkId,
  change,
}: {
  checkId: string;
  change: ReviewedContractChange;
}) {
  const style = VERDICT_STYLE[change.verdict] ?? VERDICT_STYLE.not_used;
  return (
    <li className={cn('space-y-2 rounded-md border border-l-4 p-3', style.rail)}>
      <div className='flex items-start justify-between gap-2'>
        <div className='min-w-0'>
          <p className='break-all font-mono text-xs'>{`${change.method} ${change.pathTemplate}`}</p>
          <p className='break-all text-sm font-medium'>{change.field}</p>
          <p className='text-2xs text-muted-foreground'>
            {change.changeType}
            {change.breaking && ' (breaking change)'}
          </p>
        </div>
        <Badge variant={style.variant} className='shrink-0 gap-1'>
          <VerdictIcon verdict={change.verdict} />
          {verdictLabel(change.verdict)}
        </Badge>
      </div>
      <div className='space-y-0.5'>
        <p className='text-xs font-medium'>Your requests that use it</p>
        {change.matches.length === 0 ? (
          <p className='text-xs text-muted-foreground'>None.</p>
        ) : (
          <ul className='space-y-0.5'>
            {change.matches.map((m) => (
              <li key={`${m.collection}/${m.name}`} className='break-words text-xs'>
                <span className='text-muted-foreground'>{`${m.collection} / `}</span>
                {m.name}
              </li>
            ))}
          </ul>
        )}
      </div>
      <ContractCheckChangeDiff checkId={checkId} change={change} />
    </li>
  );
}
