import { ChevronDown, ChevronRight, LayoutDashboard, Pencil, ShieldCheck, X } from 'lucide-react';
import { useRef, useState } from 'react';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import { Input } from '@/components/ui/input';
import type { Workspace } from '@/lib/tauri-api';
import { useCloseWorkspace, useRenameWorkspace } from '@/lib/queries/workspace-queries';
import { usePaneStore } from '@/stores/pane-store';

interface WorkspaceSectionProps {
  workspace: Workspace;
  children: React.ReactNode;
  collectionCount: number;
}

export function WorkspaceSection({ workspace, children, collectionCount }: WorkspaceSectionProps) {
  const [expanded, setExpanded] = useState(true);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);
  const renameMutation = useRenameWorkspace();
  const closeMutation = useCloseWorkspace();

  const handleOpenWorkspace = () => {
    usePaneStore.getState().openWorkspaceTabs(workspace.id);
  };

  const handleOpenAudit = () => {
    usePaneStore.getState().openWorkspaceTabs(workspace.id, 'audit');
  };

  const handleRename = (newName: string) => {
    if (newName !== workspace.name) {
      renameMutation.mutate({ id: workspace.id, newName });
    }
  };

  const startRenaming = () => {
    setRenameValue(workspace.name);
    setIsRenaming(true);
    // Focus after state flushes.
    setTimeout(() => inputRef.current?.focus(), 0);
  };

  const commitRename = () => {
    const trimmed = renameValue.trim();
    if (trimmed) {
      handleRename(trimmed);
    }
    setIsRenaming(false);
  };

  const handleClose = () => {
    closeMutation.mutate(workspace.id);
  };

  return (
    <div>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div className='group flex items-center gap-1 px-2 py-1 rounded-md hover:bg-accent cursor-pointer'>
            {/* Chevron toggle. */}
            <button
              type='button'
              onClick={(e) => {
                e.stopPropagation();
                setExpanded((prev) => !prev);
              }}
              className='flex items-center justify-center'
            >
              {expanded ? (
                <ChevronDown className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
              ) : (
                <ChevronRight className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
              )}
            </button>

            {/* Workspace icon. */}
            <LayoutDashboard className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />

            {/* Workspace name or inline rename input. */}
            {isRenaming ? (
              <Input
                ref={inputRef}
                value={renameValue}
                onChange={(e) => setRenameValue(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') {
                    e.preventDefault();
                    commitRename();
                  }
                  if (e.key === 'Escape') {
                    setIsRenaming(false);
                  }
                }}
                onBlur={commitRename}
                className='h-5 text-xs px-1 py-0 flex-1'
                onClick={(e) => e.stopPropagation()}
              />
            ) : (
              <button
                type='button'
                className='flex-1 truncate text-sm font-medium text-left bg-transparent border-0 p-0 cursor-pointer'
                onClick={handleOpenWorkspace}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    handleOpenWorkspace();
                  }
                }}
              >
                {workspace.name}
              </button>
            )}

            {/* Audit log shortcut. */}
            <button
              type='button'
              onClick={(e) => {
                e.stopPropagation();
                handleOpenAudit();
              }}
              className='opacity-0 group-hover:opacity-100 focus:opacity-100 text-muted-foreground hover:text-foreground rounded-sm p-0.5 transition-opacity'
              title='Open audit log'
              aria-label='Open audit log'
            >
              <ShieldCheck className='h-3.5 w-3.5' />
            </button>

            {/* Collection count. */}
            <span className='text-xs text-muted-foreground'>{collectionCount}</span>
          </div>
        </ContextMenuTrigger>

        <ContextMenuContent>
          <ContextMenuItem onSelect={handleOpenWorkspace}>
            <LayoutDashboard className='mr-2 h-3.5 w-3.5' /> Open workspace home
          </ContextMenuItem>
          <ContextMenuItem onSelect={handleOpenAudit}>
            <ShieldCheck className='mr-2 h-3.5 w-3.5' /> Open audit log
          </ContextMenuItem>
          <ContextMenuItem onSelect={startRenaming}>
            <Pencil className='mr-2 h-3.5 w-3.5' /> Rename workspace
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem
            className='text-destructive focus:text-destructive'
            disabled={workspace.id === 'default'}
            onSelect={handleClose}
          >
            <X className='mr-2 h-3.5 w-3.5' /> Close workspace
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      {/* Collapsible children. */}
      {expanded && <div className='pl-3'>{children}</div>}
    </div>
  );
}
