import { cn } from '@/lib/utils';
import type { ChangeKind } from '@/types/contracts';

interface ChangeChipProps {
  kind: ChangeKind;
  className?: string;
}

const styles: Record<ChangeKind, string> = {
  add: 'bg-[hsl(var(--success)/0.12)] text-[hsl(var(--success))] border-[hsl(var(--success)/0.25)]',
  remove:
    'bg-[hsl(var(--destructive)/0.12)] text-[hsl(var(--destructive))] border-[hsl(var(--destructive)/0.25)]',
  modify:
    'bg-[hsl(var(--warning)/0.12)] text-[hsl(var(--warning))] border-[hsl(var(--warning)/0.25)]',
};

const labels: Record<ChangeKind, string> = {
  add: '+add',
  remove: '−rem',
  modify: '~mod',
};

export function ChangeChip({ kind, className }: ChangeChipProps) {
  return (
    <span
      className={cn(
        'inline-flex items-center px-1.5 py-0.5 rounded-[3px] border text-[10px] font-mono font-semibold shrink-0',
        styles[kind],
        className,
      )}
    >
      {labels[kind]}
    </span>
  );
}
