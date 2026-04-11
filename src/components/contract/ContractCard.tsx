import { Trash2 } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Separator } from '@/components/ui/separator';
import type { Contract } from '@/lib/tauri-api';
import { useContractStore } from '@/stores/contract-store';

interface ContractCardProps {
  contract: Contract;
  collectionRoot: string;
  preview?: boolean;
  onViewChangelog?: () => void;
  onEdit?: () => void;
  onDelete?: () => void;
}

export function ContractCard({
  contract,
  collectionRoot: _collectionRoot,
  preview = false,
  onViewChangelog,
  onEdit,
  onDelete,
}: ContractCardProps) {
  const contractStatus = useContractStore((s) => s.contractStatus);
  const changelogs = useContractStore((s) => s.changelogs);
  const status = contractStatus(contract);

  const statusVariant =
    status === 'expired' ? 'destructive' : status === 'expiring' ? 'warning' : 'default';

  const statusLabel =
    status === 'expired' ? 'Expired' : status === 'expiring' ? 'Expiring soon' : 'Active';

  const scopeLabel =
    contract.scope.type === 'collection'
      ? 'Entire collection'
      : contract.scope.type === 'folder'
        ? // eslint-disable-next-line @typescript-eslint/no-explicit-any
          `Folder: ${(contract.scope as any).rel_path}`
        : // eslint-disable-next-line @typescript-eslint/no-explicit-any
          `Request: ${(contract.scope as any).rel_path}`;

  const changeCount = changelogs[contract.id]?.entries.length ?? 0;

  return (
    <div
      className={[
        'rounded-lg border bg-card p-4 space-y-3 transition-colors',
        preview ? '' : 'hover:border-primary/40',
      ].join(' ')}
    >
      {/* Header row: title + status chip */}
      <div className='flex items-start justify-between gap-2'>
        <div className='min-w-0'>
          <p className='text-sm font-medium text-foreground truncate'>{contract.title}</p>
          <p className='text-xs text-muted-foreground mt-0.5'>
            {contract.project}
            {contract.version ? ` · ${contract.version}` : ''}
          </p>
        </div>
        <Badge variant={statusVariant} className='shrink-0 text-xs'>
          {statusLabel}
        </Badge>
      </div>

      {/* Parties: pill badges with coloured dots */}
      <div className='flex items-center gap-2 flex-wrap'>
        <span className='inline-flex items-center gap-1.5 bg-secondary rounded-full px-2.5 py-1 text-xs'>
          <span className='w-2 h-2 rounded-full bg-violet-500 shrink-0' />
          {contract.provider}
        </span>
        <span className='text-muted-foreground text-xs'>→</span>
        <span className='inline-flex items-center gap-1.5 bg-secondary rounded-full px-2.5 py-1 text-xs'>
          <span className='w-2 h-2 rounded-full bg-emerald-500 shrink-0' />
          {contract.consumer}
        </span>
      </div>

      {/* Date range */}
      <div className='flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground'>
        <span>Effective {contract.effectiveDate}</span>
        {contract.expiryDate && <span>Expires {contract.expiryDate}</span>}
        {!contract.expiryDate && <span>No expiry</span>}
      </div>

      {/* Scope badge */}
      <div>
        <span className='inline-block text-xs bg-secondary text-muted-foreground rounded-full px-2.5 py-0.5'>
          {scopeLabel}
        </span>
      </div>

      {/* Footer action row — hidden in preview mode */}
      {!preview && (
        <>
          <Separator />
          <div className='flex items-center justify-between'>
            <span className='text-xs text-muted-foreground'>
              {changeCount === 0
                ? 'No changes recorded'
                : `${changeCount} change${changeCount === 1 ? '' : 's'} logged`}
            </span>
            <div className='flex items-center gap-1'>
              {onViewChangelog && (
                <Button variant='ghost' size='sm' className='h-7 text-xs' onClick={onViewChangelog}>
                  View changelog
                </Button>
              )}
              {onEdit && (
                <Button variant='ghost' size='sm' className='h-7 text-xs' onClick={onEdit}>
                  Edit
                </Button>
              )}
              {onDelete && (
                <Button
                  variant='ghost'
                  size='sm'
                  className='h-7 w-7 p-0 text-muted-foreground hover:text-destructive'
                  onClick={onDelete}
                >
                  <Trash2 className='h-3.5 w-3.5' />
                </Button>
              )}
            </div>
          </div>
        </>
      )}
    </div>
  );
}
