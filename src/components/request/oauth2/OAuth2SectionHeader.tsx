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
      {Icon && <Icon className='h-3.5 w-3.5 text-foreground/60 shrink-0' aria-hidden='true' />}
      <span className='text-xs font-semibold uppercase tracking-[0.06em] text-foreground/70'>
        {title}
      </span>
      {hint && <span className='ml-auto text-xs text-muted-foreground truncate'>{hint}</span>}
    </div>
  );
}
