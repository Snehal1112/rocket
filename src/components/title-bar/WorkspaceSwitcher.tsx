import { useState } from "react";
import {
  Check,
  ChevronDown,
  FolderOpen,
  MoreHorizontal,
  Pin,
  Plus,
} from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { openFolderPicker } from "@/lib/tauri-api";
import { useWorkspaceStore } from "@/stores/workspace-store";
import { CreateWorkspaceDialog } from "@/components/workspace/CreateWorkspaceDialog";
import { RenameWorkspaceDialog } from "@/components/workspace/RenameWorkspaceDialog";

type DialogTarget = { id: string; name: string };

export function WorkspaceSwitcher() {
  const workspaces = useWorkspaceStore((s) => s.workspaces);
  const activeId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const switchWorkspace = useWorkspaceStore((s) => s.switchWorkspace);
  const closeWorkspace = useWorkspaceStore((s) => s.closeWorkspace);
  const deleteWorkspace = useWorkspaceStore((s) => s.deleteWorkspace);

  const [createOpen, setCreateOpen] = useState(false);
  const [renameTarget, setRenameTarget] = useState<DialogTarget | null>(null);
  const [closeTarget, setCloseTarget] = useState<DialogTarget | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<DialogTarget | null>(null);

  const active = workspaces.find((w) => w.id === activeId);
  const canCloseOrDelete = workspaces.length > 1;

  const pinned = workspaces
    .filter((w) => w.pinned)
    .sort((a, b) => a.name.localeCompare(b.name));
  const unpinned = workspaces
    .filter((w) => !w.pinned)
    .sort((a, b) => a.name.localeCompare(b.name));

  const renderWorkspaceRow = (ws: (typeof workspaces)[number]) => (
    <div key={ws.id} className="flex items-center group">
      <DropdownMenuItem
        className="flex-1 gap-2"
        onSelect={() => {
          if (ws.id !== activeId) void switchWorkspace(ws.id);
        }}
      >
        <Check
          className="h-3.5 w-3.5 shrink-0"
          style={{ opacity: ws.id === activeId ? 1 : 0 }}
        />
        <span className="flex-1 truncate">{ws.name}</span>
      </DropdownMenuItem>

      {/* Pin/unpin toggle button */}
      <Button
        variant="ghost"
        size="icon"
        className="h-5 w-5 shrink-0 opacity-0 group-hover:opacity-100"
        onClick={(e) => {
          e.stopPropagation();
          if (ws.pinned) {
            useWorkspaceStore.getState().unpinWorkspace(ws.id);
          } else {
            useWorkspaceStore.getState().pinWorkspace(ws.id);
          }
        }}
      >
        <Pin className={cn("h-3.5 w-3.5", ws.pinned && "fill-current")} />
      </Button>

      {/* Per-workspace context menu */}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 shrink-0 opacity-0 group-hover:opacity-100 mr-1"
            onClick={(e) => e.stopPropagation()}
          >
            <MoreHorizontal className="h-3.5 w-3.5" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-36">
          <DropdownMenuItem
            onSelect={() => setRenameTarget({ id: ws.id, name: ws.name })}
          >
            Rename
          </DropdownMenuItem>
          <DropdownMenuItem
            onSelect={() => setCloseTarget({ id: ws.id, name: ws.name })}
            disabled={!canCloseOrDelete}
          >
            Close
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem
            className="text-destructive focus:text-destructive"
            onSelect={() => setDeleteTarget({ id: ws.id, name: ws.name })}
            disabled={ws.id === "default" || !canCloseOrDelete}
          >
            Delete
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="outline"
            size="sm"
            className="gap-1.5 font-medium max-w-[200px]"
          >
            <span className="truncate">
              {active?.name ?? "Select workspace"}
            </span>
            <ChevronDown className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          </Button>
        </DropdownMenuTrigger>

        <DropdownMenuContent
          align="center"
          className="min-w-[220px] bg-card/50 backdrop-blur-sm border border-border/70"
        >
          {pinned.map(renderWorkspaceRow)}
          {pinned.length > 0 && unpinned.length > 0 && (
            <DropdownMenuSeparator />
          )}
          {unpinned.map(renderWorkspaceRow)}

          <DropdownMenuSeparator />
          <DropdownMenuItem onSelect={() => setCreateOpen(true)}>
            <Plus className="h-3.5 w-3.5 mr-2" />
            New workspace
          </DropdownMenuItem>
          <DropdownMenuItem
            onSelect={async () => {
              const path = await openFolderPicker();
              if (path) {
                try {
                  await useWorkspaceStore
                    .getState()
                    .openWorkspaceFromDisk(path);
                } catch (err) {
                  console.error("Failed to open workspace:", err);
                }
              }
            }}
          >
            <FolderOpen className="h-3.5 w-3.5 mr-2" />
            Open workspace
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      {/* Create dialog */}
      <CreateWorkspaceDialog open={createOpen} onOpenChange={setCreateOpen} />

      {/* Rename dialog */}
      {renameTarget && (
        <RenameWorkspaceDialog
          open={!!renameTarget}
          onOpenChange={(o) => {
            if (!o) setRenameTarget(null);
          }}
          workspaceId={renameTarget.id}
          currentName={renameTarget.name}
        />
      )}

      {/* Close confirmation */}
      <AlertDialog
        open={!!closeTarget}
        onOpenChange={(o) => {
          if (!o) setCloseTarget(null);
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Close workspace</AlertDialogTitle>
            <AlertDialogDescription>
              Remove &quot;{closeTarget?.name}&quot; from Rocket? The files on
              disk will not be affected.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                void closeWorkspace(closeTarget!.id);
                setCloseTarget(null);
              }}
            >
              Close workspace
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Delete confirmation */}
      <AlertDialog
        open={!!deleteTarget}
        onOpenChange={(o) => {
          if (!o) setDeleteTarget(null);
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete workspace</AlertDialogTitle>
            <AlertDialogDescription>
              Permanently delete &quot;{deleteTarget?.name}&quot; and all its
              files from disk? This cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
              onClick={() => {
                void deleteWorkspace(deleteTarget!.id);
                setDeleteTarget(null);
              }}
            >
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
