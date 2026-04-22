import type { LucideIcon } from 'lucide-react';
import type { ReactNode } from 'react';

interface OAuth2SectionHeaderProps {
  icon?: LucideIcon;
  title: string;
  hint?: ReactNode;
  className?: string;
}

export function OAuth2SectionHeader({
  icon: Icon,
  title,
  hint,
  className = '',
}: OAuth2SectionHeaderProps) {
  return (
    <div className={`flex items-center gap-2 ${className}`}>
      {Icon && <Icon className='h-3 w-3 text-muted-foreground' />}
      <span className='text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground'>
        {title}
      </span>
      {hint && <span className='ml-auto text-2xs text-muted-foreground/70 truncate'>{hint}</span>}
    </div>
  );
}
