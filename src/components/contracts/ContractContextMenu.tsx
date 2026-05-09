import {
  CheckCircle,
  Copy,
  ExternalLink,
  FileDown,
  Link,
  PauseCircle,
  Pencil,
  PlayCircle,
  RefreshCw,
  Send,
  Trash2,
  XCircle,
} from 'lucide-react';
import { useState } from 'react';
import { toast } from 'sonner';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import { saveContractAsOpenApi } from '@/lib/contracts/exportOpenApi';
import type { Contract } from '@/types/contracts';
import type { ContractAction } from './ContractCard';

interface ContractContextMenuProps {
  contract: Contract;
  collectionRoot: string;
  onAction: (action: ContractAction, id: string) => void;
  children: React.ReactNode;
}

export function ContractContextMenu({ contract, collectionRoot, onAction, children }: ContractContextMenuProps) {
  const [deleteOpen, setDeleteOpen] = useState(false);
  const s = contract.status;

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
        <ContextMenuContent className='w-52'>
          {/* 1 — Open contract (detail view, future milestone) */}
          <ContextMenuItem onSelect={() => onAction('open', contract.id)}>
            <ExternalLink className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
            Open contract
          </ContextMenuItem>
          {/* 2 — Edit */}
          <ContextMenuItem onSelect={() => onAction('edit', contract.id)}>
            <Pencil className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
            Edit
          </ContextMenuItem>

          <ContextMenuSeparator />

          {/* 3 — Duplicate */}
          <ContextMenuItem onSelect={() => onAction('duplicate', contract.id)}>
            <Copy className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
            Duplicate
          </ContextMenuItem>
          {/* 4 — Export as OpenAPI */}
          <ContextMenuItem
            onSelect={() => {
              saveContractAsOpenApi(collectionRoot, contract.id, contract.name).catch((err) => {
                toast.error(`Export failed: ${err}`);
              });
            }}
          >
            <FileDown className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
            Export as OpenAPI
          </ContextMenuItem>
          {/* 5 — Copy contract link */}
          <ContextMenuItem
            onSelect={() => navigator.clipboard.writeText(`rocketapi://contract/${contract.id}`)}
          >
            <Link className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
            Copy contract link
          </ContextMenuItem>

          <ContextMenuSeparator />

          {/* 6a — Approve / Reject (in_review only) */}
          {s === 'in_review' && (
            <>
              <ContextMenuItem onSelect={() => onAction('approve', contract.id)}>
                <CheckCircle className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
                Approve
              </ContextMenuItem>
              <ContextMenuItem onSelect={() => onAction('reject', contract.id)}>
                <XCircle className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
                Reject
              </ContextMenuItem>
              <ContextMenuSeparator />
            </>
          )}

          {/* 6b — Send for review */}
          {(['active', 'drift', 'breach', 'expiring_in_30_days'] as const).includes(
            s as 'active' | 'drift' | 'breach' | 'expiring_in_30_days',
          ) && (
            <ContextMenuItem onSelect={() => onAction('send_for_review', contract.id)}>
              <Send className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
              Send for review
            </ContextMenuItem>
          )}

          {/* 7 — Pause monitoring */}
          {(['active', 'drift', 'breach', 'expiring_in_30_days'] as const).includes(
            s as 'active' | 'drift' | 'breach' | 'expiring_in_30_days',
          ) && (
            <ContextMenuItem onSelect={() => onAction('pause', contract.id)}>
              <PauseCircle className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
              Pause monitoring
            </ContextMenuItem>
          )}

          {/* 8 — Resume */}
          {s === 'paused' && (
            <ContextMenuItem onSelect={() => onAction('resume', contract.id)}>
              <PlayCircle className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
              Resume
            </ContextMenuItem>
          )}

          {/* 9 — Renew */}
          {s === 'expired' && (
            <ContextMenuItem onSelect={() => onAction('renew', contract.id)}>
              <RefreshCw className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
              Renew
            </ContextMenuItem>
          )}

          <ContextMenuSeparator />

          {/* 10 — Delete */}
          <ContextMenuItem
            onSelect={() => setDeleteOpen(true)}
            className='text-destructive focus:text-destructive'
          >
            <Trash2 className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
            Delete
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      {/* Delete confirmation dialog */}
      <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete contract?</AlertDialogTitle>
            <AlertDialogDescription>
              <strong>{contract.name}</strong> and all its changelog entries will be permanently
              deleted. This action cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                setDeleteOpen(false);
                onAction('delete', contract.id);
              }}
              className='bg-destructive text-destructive-foreground hover:bg-destructive/90'
            >
              Delete contract
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
