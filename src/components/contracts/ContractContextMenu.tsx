import {
  CheckCircle,
  Copy,
  ExternalLink,
  FileDown,
  History,
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
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { saveContractAsOpenApi } from '@/lib/contracts/exportOpenApi';
import { track } from '@/lib/telemetry';
import { useDrawerStore } from '@/stores/contracts/drawerSlice';
import type { Contract } from '@/types/contracts';
import type { ContractAction } from './ContractCard';

interface ContractMenuProps {
  contract: Contract;
  collectionRoot: string;
  onAction: (action: ContractAction, id: string) => void;
  children: React.ReactNode;
}

/**
 * Action menu items shared between right-click context menu and the
 * `...` kebab dropdown. Parameterised over `Item` / `Separator` so the
 * same JSX works for both Radix `ContextMenu` and `DropdownMenu`.
 */
function renderItems(
  contract: Contract,
  collectionRoot: string,
  onAction: (action: ContractAction, id: string) => void,
  openDelete: () => void,
  openDrawer: (contractId: string) => void,
  Item: typeof ContextMenuItem | typeof DropdownMenuItem,
  Separator: typeof ContextMenuSeparator | typeof DropdownMenuSeparator,
) {
  const s = contract.status;
  return (
    <>
      <Item onSelect={() => onAction('open', contract.id)}>
        <ExternalLink className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
        Open contract
      </Item>
      <Item onSelect={() => onAction('edit', contract.id)}>
        <Pencil className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
        Edit
      </Item>
      <Item
        onSelect={() => {
          try {
            track('contracts.changelog_drawer_opened', {
              contractId: contract.id,
              source: 'context_menu',
            });
          } catch {
            /* noop */
          }
          openDrawer(contract.id);
        }}
      >
        <History className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
        Show changelog
      </Item>

      <Separator />

      <Item onSelect={() => onAction('duplicate', contract.id)}>
        <Copy className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
        Duplicate
      </Item>
      <Item
        onSelect={async () => {
          try {
            await saveContractAsOpenApi(collectionRoot, contract.id, contract.name);
          } catch (err) {
            console.error('[ContractMenu] export failed:', err);
          }
        }}
      >
        <FileDown className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
        Export as OpenAPI
      </Item>
      <Item onSelect={() => navigator.clipboard.writeText(`rocketapi://contract/${contract.id}`)}>
        <Link className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
        Copy contract link
      </Item>

      <Separator />

      {s === 'in_review' && (
        <>
          <Item onSelect={() => onAction('approve', contract.id)}>
            <CheckCircle className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
            Approve
          </Item>
          <Item onSelect={() => onAction('reject', contract.id)}>
            <XCircle className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
            Reject
          </Item>
          <Separator />
        </>
      )}

      {(['active', 'drift', 'breach', 'expiring_in_30_days'] as const).includes(
        s as 'active' | 'drift' | 'breach' | 'expiring_in_30_days',
      ) && (
        <Item onSelect={() => onAction('send_for_review', contract.id)}>
          <Send className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
          Send for review
        </Item>
      )}

      {(['active', 'drift', 'breach', 'expiring_in_30_days'] as const).includes(
        s as 'active' | 'drift' | 'breach' | 'expiring_in_30_days',
      ) && (
        <Item onSelect={() => onAction('pause', contract.id)}>
          <PauseCircle className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
          Pause monitoring
        </Item>
      )}

      {s === 'paused' && (
        <Item onSelect={() => onAction('resume', contract.id)}>
          <PlayCircle className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
          Resume
        </Item>
      )}

      {s === 'expired' && (
        <Item onSelect={() => onAction('renew', contract.id)}>
          <RefreshCw className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
          Renew
        </Item>
      )}

      <Separator />

      <Item onSelect={openDelete} className='text-destructive focus:text-destructive'>
        <Trash2 className='h-3.5 w-3.5 mr-2' aria-hidden='true' />
        Delete
      </Item>
    </>
  );
}

function DeleteDialog({
  open,
  onOpenChange,
  contract,
  onAction,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  contract: Contract;
  onAction: (action: ContractAction, id: string) => void;
}) {
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
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
              onOpenChange(false);
              onAction('delete', contract.id);
            }}
            className='bg-destructive text-destructive-foreground hover:bg-destructive/90'
          >
            Delete contract
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

/** Right-click context menu wrapper. Children become the right-clickable area. */
export function ContractContextMenu({
  contract,
  collectionRoot,
  onAction,
  children,
}: ContractMenuProps) {
  const [deleteOpen, setDeleteOpen] = useState(false);
  const openDrawer = useDrawerStore((s) => s.open);

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
        <ContextMenuContent className='w-52'>
          {renderItems(
            contract,
            collectionRoot,
            onAction,
            () => setDeleteOpen(true),
            openDrawer,
            ContextMenuItem,
            ContextMenuSeparator,
          )}
        </ContextMenuContent>
      </ContextMenu>
      <DeleteDialog
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        contract={contract}
        onAction={onAction}
      />
    </>
  );
}

/** Left-click kebab dropdown wrapper. Children become the click target. */
export function ContractDropdownMenu({
  contract,
  collectionRoot,
  onAction,
  children,
}: ContractMenuProps) {
  const [deleteOpen, setDeleteOpen] = useState(false);
  const openDrawer = useDrawerStore((s) => s.open);

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>{children}</DropdownMenuTrigger>
        <DropdownMenuContent align='end' className='w-52'>
          {renderItems(
            contract,
            collectionRoot,
            onAction,
            () => setDeleteOpen(true),
            openDrawer,
            DropdownMenuItem,
            DropdownMenuSeparator,
          )}
        </DropdownMenuContent>
      </DropdownMenu>
      <DeleteDialog
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        contract={contract}
        onAction={onAction}
      />
    </>
  );
}
