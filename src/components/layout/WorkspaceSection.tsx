import { useState, useRef } from 'react'
import { ChevronDown, ChevronRight, LayoutDashboard, Pencil, X } from 'lucide-react'
import { Input } from '@/components/ui/input'
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
  const [isRenaming, setIsRenaming] = useState(false)
  const [renameValue, setRenameValue] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)

  const handleOpenWorkspace = () => {
    usePaneStore.getState().openWorkspaceTabs(workspace.id)
  }

  const handleRename = (newName: string) => {
    if (newName !== workspace.name) {
      void useWorkspaceStore.getState().renameWorkspace(workspace.id, newName)
    }
  }

  const startRenaming = () => {
    setRenameValue(workspace.name)
    setIsRenaming(true)
    // Focus after state flushes.
    setTimeout(() => inputRef.current?.focus(), 0)
  }

  const commitRename = () => {
    const trimmed = renameValue.trim()
    if (trimmed) {
      handleRename(trimmed)
    }
    setIsRenaming(false)
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
            <LayoutDashboard className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />

            {/* Workspace name or inline rename input. */}
            {isRenaming ? (
              <Input
                ref={inputRef}
                value={renameValue}
                onChange={(e) => setRenameValue(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') {
                    e.preventDefault()
                    commitRename()
                  }
                  if (e.key === 'Escape') {
                    setIsRenaming(false)
                  }
                }}
                onBlur={commitRename}
                className="h-5 text-xs px-1 py-0 flex-1"
                onClick={(e) => e.stopPropagation()}
              />
            ) : (
              <span
                className="flex-1 truncate text-sm font-medium"
                onClick={handleOpenWorkspace}
              >
                {workspace.name}
              </span>
            )}

            {/* Collection count. */}
            <span className="text-xs text-muted-foreground">{collectionCount}</span>
          </div>
        </ContextMenuTrigger>

        <ContextMenuContent>
          <ContextMenuItem onSelect={handleOpenWorkspace}>
            <LayoutDashboard className="mr-2 h-3.5 w-3.5" /> Open workspace home
          </ContextMenuItem>
          <ContextMenuItem onSelect={startRenaming}>
            <Pencil className="mr-2 h-3.5 w-3.5" /> Rename workspace
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem
            className="text-destructive focus:text-destructive"
            disabled={workspace.id === 'default'}
            onSelect={handleClose}
          >
            <X className="mr-2 h-3.5 w-3.5" /> Close workspace
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      {/* Collapsible children. */}
      {expanded && <div className="pl-3">{children}</div>}
    </div>
  )
}
