import type { ReactNode } from 'react';
import { cn } from '@/lib/utils';

interface SectionLabelProps {
  /** A heading by default. Use `p` for a title that is not part of the outline. */
  as?: 'h3' | 'p';
  className?: string;
  children: ReactNode;
}

/** The small upper-case label above a group of rows. */
export function SectionLabel({ as: Tag = 'h3', className, children }: SectionLabelProps) {
  return (
    <Tag
      className={cn(
        'text-[11px] font-semibold uppercase tracking-wider text-muted-foreground',
        className,
      )}
    >
      {children}
    </Tag>
  );
}
