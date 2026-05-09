import type { ReactNode } from 'react';
import { cn } from '@/lib/utils';

interface MetaItemProps {
  icon: ReactNode;
  label?: string;
  value: string;
  danger?: boolean;
  warning?: boolean;
}

/**
 * Icon + optional label + value row used in the card meta section.
 * Colors default to muted-foreground; danger/warning override the value color.
 */
export function MetaItem({ icon, label, value, danger, warning }: MetaItemProps) {
  return (
    <div
      className={cn(
        'flex items-center gap-1 text-xs text-muted-foreground',
        danger && 'text-[hsl(var(--destructive))]',
        warning && 'text-[hsl(var(--warning))]',
      )}
    >
      <span aria-hidden='true' className='shrink-0'>
        {icon}
      </span>
      {label && <span className='shrink-0'>{label}</span>}
      <span
        className={cn('font-medium', !danger && !warning && 'text-foreground')}
      >
        {value}
      </span>
    </div>
  );
}
