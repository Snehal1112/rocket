import { FileLock } from 'lucide-react';
import type { KeyboardEvent, MouseEvent } from 'react';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import type { Contract } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { useContractStore } from '@/stores/contract-store';
import { usePaneStore } from '@/stores/pane-store';

interface ContractBadgeProps {
  contracts: Contract[];
  collectionName: string;
  collectionRoot: string;
}

/**
 * Small lock icon shown next to a sidebar item covered by one or more
 * active contracts. Clicking opens the ContractTab for this collection.
 *
 * Visual semantics:
 * - expired   → destructive colour
 * - expiring  → warning colour (≤30 days to expiry)
 * - active    → muted foreground
 */
export function ContractBadge({ contracts, collectionName, collectionRoot }: ContractBadgeProps) {
  const contractStatus = useContractStore((s) => s.contractStatus);
  const openContractTab = usePaneStore((s) => s.openContractTab);

  if (contracts.length === 0) return null;

  const primary = contracts[0];
  const status = contractStatus(primary);

  const iconColor =
    status === 'expired'
      ? 'text-destructive'
      : status === 'expiring'
        ? 'text-warning'
        : 'text-muted-foreground';

  const handleClick = (event: MouseEvent<HTMLDivElement>) => {
    // Prevent the sidebar row click from selecting the collection.
    event.stopPropagation();
    openContractTab(collectionName, collectionRoot);
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'Enter' || event.key === '') {
      event.preventDefault();
      event.stopPropagation();
      openContractTab(collectionName, collectionRoot);
    }
  };

  return (
    <TooltipProvider delayDuration={200}>
      <Tooltip>
        <TooltipTrigger asChild>
          <div
            role='button'
            tabIndex={0}
            onClick={handleClick}
            onKeyDown={handleKeyDown}
            className={cn(
              'inline-flex items-center justify-center h-4 w-4 rounded-sm hover:bg-accent',
              iconColor,
            )}
            aria-label='Manage contracts'
          >
            <FileLock className='h-4 w-4 shrink-0' />
          </div>
        </TooltipTrigger>
        <TooltipContent side='right'>
          <p className='text-xs font-medium'>{primary.title}</p>
          <p className='text-xs text-primary-foreground/80'>
            {primary.provider.name} → {primary.consumers[0]?.name ?? '—'}
          </p>
          {contracts.length > 1 && (
            <p className='text-xs text-primary-foreground/60'>+{contracts.length - 1} more</p>
          )}
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
