import { Button } from '@/components/ui/button';
import type { Contract } from '@/types/contracts';
import type { ContractAction } from '../ContractCard';

interface PrimaryActionProps {
  contract: Pick<Contract, 'id' | 'status'>;
  onAction: (action: ContractAction, id: string) => void;
}

/**
 * Context-sensitive primary CTA shown in the card footer on hover.
 * Returns null for active/in_review/expiring (no single primary action needed).
 */
export function PrimaryAction({ contract, onAction }: PrimaryActionProps) {
  const stop = (e: React.MouseEvent) => e.stopPropagation();

  switch (contract.status) {
    case 'drift':
    case 'breach':
      return (
        <Button
          variant='outline'
          size='sm'
          className='h-7 text-xs'
          onClick={(e) => {
            stop(e);
            onAction('resign', contract.id);
          }}
        >
          Re-sign
        </Button>
      );
    case 'draft':
      return (
        <Button
          variant='outline'
          size='sm'
          className='h-7 text-xs'
          onClick={(e) => {
            stop(e);
            onAction('publish', contract.id);
          }}
        >
          Publish
        </Button>
      );
    case 'paused':
      return (
        <Button
          variant='outline'
          size='sm'
          className='h-7 text-xs'
          onClick={(e) => {
            stop(e);
            onAction('resume', contract.id);
          }}
        >
          Resume
        </Button>
      );
    case 'expired':
      return (
        <Button
          variant='outline'
          size='sm'
          className='h-7 text-xs'
          onClick={(e) => {
            stop(e);
            onAction('renew', contract.id);
          }}
        >
          Renew
        </Button>
      );
    default:
      return null;
  }
}
