import { FileText, MoreHorizontal, Pencil, Trash2 } from 'lucide-react';
import { useRef, useState } from 'react';
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
import { Button } from '@/components/ui/button';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { TREE_ICON, TREE_ICON_STROKE, TreeItem, TreeItemContent } from '@/components/ui/tree';
import { deleteRequestExample, renameRequestExample } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

interface ExampleNodeProps {
  collectionName: string;
  /** Collection-relative path of the request that owns the example. */
  requestPath: string;
  name: string;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export function ExampleNode({ collectionName, requestPath, name }: ExampleNodeProps) {
  const openExampleTab = usePaneStore((s) => s.openExampleTab);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState(name);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const renameInFlight = useRef(false);
  // Set on Escape to block the blur that fires when the Input unmounts.
  const renameCancelled = useRef(false);

  const startRename = () => {
    setRenameValue(name);
    // Wait for the menu to release focus, or the input blurs at once.
    setTimeout(() => setIsRenaming(true), 0);
  };

  const handleRename = async () => {
    if (renameInFlight.current) return;
    if (renameCancelled.current) {
      renameCancelled.current = false;
      return;
    }
    const trimmed = renameValue.trim();
    if (!trimmed || trimmed === name) {
      setIsRenaming(false);
      return;
    }
    renameInFlight.current = true;
    try {
      await renameRequestExample(collectionName, requestPath, name, trimmed);
      usePaneStore.getState().retitleExampleTab(collectionName, requestPath, name, trimmed);
    } catch (err) {
      toast.error(`Could not rename "${name}": ${errorMessage(err)}`);
    } finally {
      renameInFlight.current = false;
      setIsRenaming(false);
    }
  };

  const handleDelete = async () => {
    try {
      await deleteRequestExample(collectionName, requestPath, name);
      usePaneStore.getState().closeExampleTabs(collectionName, requestPath, name);
    } catch (err) {
      toast.error(`Could not delete "${name}": ${errorMessage(err)}`);
    }
  };

  return (
    // The wrapper keeps the request row's menu and drag from reacting to this row.
    // biome-ignore lint/a11y/noStaticElementInteractions: it only stops event bubbling
    <div
      className='group relative flex items-center'
      onContextMenu={(e) => e.stopPropagation()}
      onDragStart={(e) => {
        e.preventDefault();
        e.stopPropagation();
      }}
    >
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div className='flex-1 min-w-0'>
            <TreeItem value={`example-${collectionName}-${requestPath}-${name}`} className='w-full'>
              <TreeItemContent
                onClick={() => {
                  if (!isRenaming) openExampleTab(collectionName, requestPath, name);
                }}
                aria-label={`Open example ${name}`}
              >
                <FileText aria-hidden='true' strokeWidth={TREE_ICON_STROKE} className={TREE_ICON} />
                {isRenaming ? (
                  <Input
                    autoFocus
                    className='h-6 text-[13px] flex-1'
                    value={renameValue}
                    onChange={(e) => setRenameValue(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') void handleRename();
                      if (e.key === 'Escape') {
                        renameCancelled.current = true;
                        setIsRenaming(false);
                      }
                    }}
                    onBlur={() => void handleRename()}
                    onClick={(e) => e.stopPropagation()}
                  />
                ) : (
                  <span className='truncate text-foreground'>{name}</span>
                )}
              </TreeItemContent>
            </TreeItem>
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent
          className='w-40'
          // Keeps the rename input focused instead of returning focus to the row.
          onCloseAutoFocus={(e) => e.preventDefault()}
        >
          <ContextMenuItem onSelect={startRename}>
            <Pencil aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Rename
          </ContextMenuItem>
          <ContextMenuItem className='text-destructive' onSelect={() => setConfirmingDelete(true)}>
            <Trash2 aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Delete
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            type='button'
            variant='ghost'
            size='icon'
            aria-label={`Actions for ${name}`}
            className='sticky right-1 -ml-6 shrink-0 h-5 w-5 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100'
            onClick={(e) => e.stopPropagation()}
          >
            <MoreHorizontal aria-hidden='true' className='h-3 w-3' />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent
          className='w-40'
          onClick={(e) => e.stopPropagation()}
          onCloseAutoFocus={(e) => e.preventDefault()}
        >
          <DropdownMenuItem onSelect={startRename}>
            <Pencil aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Rename
          </DropdownMenuItem>
          <DropdownMenuItem className='text-destructive' onSelect={() => setConfirmingDelete(true)}>
            <Trash2 aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Delete
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      <AlertDialog open={confirmingDelete} onOpenChange={setConfirmingDelete}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Confirm Delete</AlertDialogTitle>
            <AlertDialogDescription>{`Delete example '${name}'?`}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={() => void handleDelete()}>Delete</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
