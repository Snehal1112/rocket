import { useState } from 'react'
import { ChevronDown, ChevronRight, Plus, LayoutDashboard, Pencil, X } from 'lucide-react'
import { Button } from '@/components/ui/button'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { usePaneStore } from '@/stores/pane-store'
import { useWorkspaceStore } from '@/stores/workspace-store'
import type { Workspace } from '@/lib/tauri-api'

interface WorkspaceSectionProps {
  workspace: Workspace
  children: React.ReactNode
  collectionCount: number
}

export function WorkspaceSection({ workspace, children, collectionCount }: WorkspaceSectionProps) {
  const [expanded, setExpanded] = useState(true)

  const handleOpenWorkspace = () => {
    usePaneStore.getState().openWorkspaceTabs(workspace.id, workspace.id === 'default')
  }

  const handleRename = () => {
    const newName = window.prompt('Rename workspace', workspace.name)
    if (newName && newName.trim() && newName.trim() !== workspace.name) {
      void useWorkspaceStore.getState().renameWorkspace(workspace.id, newName.trim())
    }
  }

  const handleClose = () => {
    void useWorkspaceStore.getState().closeWorkspace(workspace.id)
  }

  return (
    <div>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div
            className="group flex items-center gap-1 px-2 py-1 rounded-md hover:bg-accent cursor-pointer"
          >
            {/* Chevron toggle. */}
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation()
                setExpanded((prev) => !prev)
              }}
              className="flex items-center justify-center"
            >
              {expanded ? (
                <ChevronDown className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
              ) : (
                <ChevronRight className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
              )}
            </button>

            {/* Workspace icon. */}
            <LayoutDashboard className="h-4 w-4 shrink-0 text-muted-foreground" />

            {/* Workspace name — clicking opens workspace tabs. */}
            <span
              className="flex-1 truncate text-sm font-medium"
              onClick={handleOpenWorkspace}
            >
              {workspace.name}
            </span>

            {/* Collection count. */}
            <span className="text-xs text-muted-foreground">{collectionCount}</span>

            {/* New collection button — visible on hover. */}
            <Button
              variant="ghost"
              size="icon"
              className="h-5 w-5 opacity-0 group-hover:opacity-100"
              onClick={(e) => e.stopPropagation()}
            >
              <Plus className="h-3 w-3" />
            </Button>
          </div>
        </ContextMenuTrigger>

        <ContextMenuContent>
          <ContextMenuItem onSelect={handleOpenWorkspace}>
            <LayoutDashboard className="mr-2 h-4 w-4" /> Open workspace home
          </ContextMenuItem>
          <ContextMenuItem onSelect={handleRename}>
            <Pencil className="mr-2 h-4 w-4" /> Rename workspace
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem
            className="text-destructive focus:text-destructive"
            disabled={workspace.id === 'default'}
            onSelect={handleClose}
          >
            <X className="mr-2 h-4 w-4" /> Close workspace
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      {/* Collapsible children. */}
      {expanded && <div className="pl-3">{children}</div>}
    </div>
  )
}
