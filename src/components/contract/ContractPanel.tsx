import { Trash2 } from 'lucide-react';
import { useEffect } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Separator } from '@/components/ui/separator';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import type { ChangelogEntry, Contract } from '@/lib/tauri-api';
import { useContractStore } from '@/stores/contract-store';

interface ContractPanelProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  contract: Contract;
  collectionRoot: string;
}

/**
 * Contract detail view. Shows parties, dates, status, and the
 * append-only changelog. Also exposes a destructive "Remove contract"
 * action that deletes the contract, its snapshot, and its changelog.
 *
 * Implementation note: the design spec calls for a right-side Sheet,
 * but the shadcn/ui `sheet` primitive is not installed in this project.
 * Dialog is used as the closest available alternative with a wider
 * fixed max-width so the changelog table fits comfortably.
 */
export function ContractPanel({
  open,
  onOpenChange,
  contract,
  collectionRoot,
}: ContractPanelProps) {
  const loadChangelog = useContractStore((s) => s.loadChangelog);
  const removeContract = useContractStore((s) => s.removeContract);
  const contractStatus = useContractStore((s) => s.contractStatus);
  const changelog = useContractStore((s) => s.changelogs[contract.id]);
  const status = contractStatus(contract);

  useEffect(() => {
    if (open) {
      void loadChangelog(collectionRoot, contract.id);
    }
  }, [open, collectionRoot, contract.id, loadChangelog]);

  const handleDelete = async () => {
    await removeContract(collectionRoot, contract.id);
    onOpenChange(false);
  };

  const statusBadgeVariant: 'default' | 'destructive' | 'warning' =
    status === 'expired' ? 'destructive' : status === 'expiring' ? 'warning' : 'default';
  const statusLabel =
    status === 'expired' ? 'Expired' : status === 'expiring' ? 'Expiring soon' : 'Active';

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='sm:max-w-2xl max-h-[85vh] overflow-y-auto'>
        <DialogHeader className='mb-4'>
          <div className='flex items-start justify-between gap-2 pr-6'>
            <DialogTitle className='text-base leading-tight'>{contract.title}</DialogTitle>
            <Badge variant={statusBadgeVariant}>{statusLabel}</Badge>
          </div>
        </DialogHeader>

        {/* Parties and metadata */}
        <div className='rounded-lg border p-4 mb-4 space-y-3 text-sm'>
          <div className='flex items-center gap-2 flex-wrap'>
            <div className='flex items-center gap-2 bg-secondary rounded-full px-3 py-1 text-xs'>
              <span className='w-2 h-2 rounded-full bg-[#534AB7] shrink-0' />
              <span>Provider: {contract.provider}</span>
            </div>
            <span className='text-muted-foreground text-sm'>→</span>
            <div className='flex items-center gap-2 bg-secondary rounded-full px-3 py-1 text-xs'>
              <span className='w-2 h-2 rounded-full bg-[#1D9E75] shrink-0' />
              <span>Consumer: {contract.consumer}</span>
            </div>
          </div>
          <Separator />
          <div className='flex justify-between'>
            <span className='text-muted-foreground'>Project</span>
            <span>{contract.project}</span>
          </div>
          <div className='flex justify-between'>
            <span className='text-muted-foreground'>Version</span>
            <span>{contract.version}</span>
          </div>
          <div className='flex justify-between'>
            <span className='text-muted-foreground'>Effective</span>
            <span>{contract.effectiveDate}</span>
          </div>
          {contract.expiryDate && (
            <div className='flex justify-between'>
              <span className='text-muted-foreground'>Expires</span>
              <span>{contract.expiryDate}</span>
            </div>
          )}
          <div className='flex justify-between'>
            <span className='text-muted-foreground'>Scope</span>
            <span>{formatScope(contract)}</span>
          </div>
        </div>

        {/* Changelog */}
        <p className='text-xs font-medium text-muted-foreground uppercase tracking-wide mb-2'>
          Change log
        </p>
        {!changelog || changelog.entries.length === 0 ? (
          <p className='text-sm text-muted-foreground py-4 text-center border rounded-lg'>
            No changes recorded since contract was signed.
          </p>
        ) : (
          <div className='border rounded-lg overflow-hidden mb-4'>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className='text-xs'>Date</TableHead>
                  <TableHead className='text-xs'>Field</TableHead>
                  <TableHead className='text-xs'>Type</TableHead>
                  <TableHead className='text-xs'>Old</TableHead>
                  <TableHead className='text-xs'>New</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {changelog.entries.map((entry) => (
                  <ChangelogRow
                    key={`${entry.timestamp}-${entry.field}-${entry.changeType}-${entry.newValue ?? ''}-${entry.oldValue ?? ''}`}
                    entry={entry}
                  />
                ))}
              </TableBody>
            </Table>
          </div>
        )}

        <Separator className='my-4' />
        <Button variant='destructive' size='sm' onClick={handleDelete} className='w-full'>
          <Trash2 className='h-4 w-4 mr-2' />
          Remove contract
        </Button>
      </DialogContent>
    </Dialog>
  );
}

function ChangelogRow({ entry }: { entry: ChangelogEntry }) {
  const changeVariant: 'default' | 'secondary' | 'destructive' =
    entry.changeType === 'removed'
      ? 'destructive'
      : entry.changeType === 'added'
        ? 'default'
        : 'secondary';
  return (
    <TableRow>
      <TableCell className='text-xs text-muted-foreground whitespace-nowrap'>
        {new Date(entry.timestamp).toLocaleDateString()}
      </TableCell>
      <TableCell className='text-xs font-mono'>{entry.field}</TableCell>
      <TableCell>
        <Badge variant={changeVariant} className='text-xs'>
          {entry.changeType}
        </Badge>
      </TableCell>
      <TableCell className='text-xs text-muted-foreground font-mono'>
        {entry.oldValue ?? '—'}
      </TableCell>
      <TableCell className='text-xs font-mono'>{entry.newValue ?? '—'}</TableCell>
    </TableRow>
  );
}

function formatScope(contract: Contract): string {
  const scope = contract.scope;
  if (scope.type === 'collection') return 'Entire collection';
  if (scope.type === 'folder') return `Folder: ${scope.rel_path}`;
  return `Request: ${scope.rel_path}`;
}
