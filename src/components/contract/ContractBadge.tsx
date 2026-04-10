import { Lock } from 'lucide-react';
import { type MouseEvent, useState } from 'react';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import type { Contract } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { useContractStore } from '@/stores/contract-store';
import { ContractPanel } from './ContractPanel';

interface ContractBadgeProps {
  contracts: Contract[];
  collectionRoot: string;
}

/**
 * Small lock icon shown next to a sidebar item that is covered by one
 * or more active contracts. Clicking opens the ContractPanel focused
 * on the first (and usually only) contract.
 *
 * Visual semantics:
 * - expired   → destructive colour
 * - expiring  → warning colour (≤30 days to expiry)
 * - active    → muted foreground
 */
export function ContractBadge({ contracts, collectionRoot }: ContractBadgeProps) {
  const [open, setOpen] = useState(false);
  const contractStatus = useContractStore((s) => s.contractStatus);

  if (contracts.length === 0) return null;

  const primary = contracts[0];
  const status = contractStatus(primary);

  const iconColor =
    status === 'expired'
      ? 'text-destructive'
      : status === 'expiring'
        ? 'text-warning'
        : 'text-muted-foreground';

  const handleClick = (event: MouseEvent<HTMLButtonElement>) => {
    // Prevent the sidebar row click from selecting the collection.
    event.stopPropagation();
    setOpen(true);
  };

  return (
    <>
      <TooltipProvider delayDuration={200}>
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              type='button'
              onClick={handleClick}
              className={cn(
                'inline-flex items-center justify-center h-4 w-4 rounded-sm hover:bg-accent',
                iconColor,
              )}
              aria-label={`View contract: ${primary.title}`}
            >
              <Lock className='h-3 w-3' />
            </button>
          </TooltipTrigger>
          <TooltipContent side='right'>
            <p className='text-xs font-medium'>{primary.title}</p>
            <p className='text-xs text-primary-foreground/80'>
              {primary.provider} → {primary.consumer}
            </p>
            {contracts.length > 1 && (
              <p className='text-xs text-primary-foreground/60'>+{contracts.length - 1} more</p>
            )}
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>

      <ContractPanel
        open={open}
        onOpenChange={setOpen}
        contract={primary}
        collectionRoot={collectionRoot}
      />
    </>
  );
}
