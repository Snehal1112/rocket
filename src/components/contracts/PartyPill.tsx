import { cn } from '@/lib/utils';
import type { Party, PartyRole } from '@/types/contracts';
import { PartyAvatar } from './PartyAvatar';

interface PartyPillProps {
  party: Party;
  role?: PartyRole;
  className?: string;
}

const roleLabels: Record<PartyRole, string> = { provider: 'Provider', consumer: 'Consumer' };

export function PartyPill({ party, role, className }: PartyPillProps) {
  return (
    <div
      className={cn(
        'inline-flex items-center gap-[7px] pl-[4px] pr-[10px] py-1',
        'border border-border rounded-full bg-card text-xs text-foreground',
        className,
      )}
    >
      <PartyAvatar party={party} size={20} />
      <span className='truncate max-w-[120px]'>{party.name}</span>
      {role && (
        <span className='text-[10px] text-muted-foreground font-medium shrink-0'>
          · {roleLabels[role]}
        </span>
      )}
    </div>
  );
}
