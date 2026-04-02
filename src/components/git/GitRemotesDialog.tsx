import { useEffect, useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { Pencil, Trash2, Check, X, Plus } from "lucide-react";
import { useGitStore } from "@/stores/git-store";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function GitRemotesDialog({ open, onOpenChange }: Props) {
  const { remotes, addRemote, removeRemote, setRemoteUrl, refreshRemotes } =
    useGitStore();

  const [newName, setNewName] = useState("");
  const [newUrl, setNewUrl] = useState("");
  const [editingRemote, setEditingRemote] = useState<string | null>(null);
  const [editUrl, setEditUrl] = useState("");
  const [deletingRemote, setDeletingRemote] = useState<string | null>(null);

  // Refresh the remote list each time the dialog opens.
  useEffect(() => {
    if (open) {
      refreshRemotes();
    }
  }, [open, refreshRemotes]);

  const canAdd =
    newName.trim().length > 0 &&
    !newName.includes(" ") &&
    newUrl.trim().length > 0 &&
    !remotes.some((r) => r.name === newName.trim());

  const handleAdd = async () => {
    await addRemote(newName.trim(), newUrl.trim());
    setNewName("");
    setNewUrl("");
  };

  const handleSaveEdit = async () => {
    if (!editingRemote) return;
    await setRemoteUrl(editingRemote, editUrl.trim());
    setEditingRemote(null);
  };

  const handleConfirmDelete = async () => {
    if (!deletingRemote) return;
    await removeRemote(deletingRemote);
    setDeletingRemote(null);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="[&::-webkit-scrollbar]:hidden [scrollbar-width:none]">
        <DialogHeader>
          <DialogTitle>Manage Remotes</DialogTitle>
        </DialogHeader>

        <TooltipProvider delayDuration={300}>
          <div className="space-y-3 min-w-0">
            {remotes.length === 0 ? (
              <p className="text-sm text-muted-foreground text-center py-4">
                No remotes configured.
              </p>
            ) : (
              <div className="space-y-1">
                {remotes.map((remote) => {
                  if (deletingRemote === remote.name) {
                    return (
                      <div
                        key={remote.name}
                        className="flex items-center gap-2 px-2 py-1.5 rounded-md bg-destructive/10"
                      >
                        <span className="text-sm flex-1">
                          Remove{" "}
                          <span className="font-mono font-semibold">
                            {remote.name}
                          </span>
                          ?
                        </span>
                        <Button
                          size="sm"
                          variant="destructive"
                          className="h-7 text-xs"
                          onClick={handleConfirmDelete}
                        >
                          Remove
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          className="h-7 text-xs"
                          onClick={() => setDeletingRemote(null)}
                        >
                          Cancel
                        </Button>
                      </div>
                    );
                  }

                  if (editingRemote === remote.name) {
                    return (
                      <div
                        key={remote.name}
                        className="flex items-center gap-2 px-2 py-1 min-w-0"
                      >
                        <span className="font-mono font-semibold text-sm shrink-0 max-w-[100px] truncate">
                          {remote.name}
                        </span>
                        <Input
                          value={editUrl}
                          onChange={(e) => setEditUrl(e.target.value)}
                          className="h-7 text-sm flex-1 min-w-0"
                          autoFocus
                          onKeyDown={(e) => {
                            if (e.key === "Enter") handleSaveEdit();
                            if (e.key === "Escape") setEditingRemote(null);
                          }}
                        />
                        <Button
                          size="sm"
                          variant="ghost"
                          className="h-7 w-7 p-0 shrink-0"
                          onClick={handleSaveEdit}
                        >
                          <Check className="h-3.5 w-3.5" />
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          className="h-7 w-7 p-0 shrink-0"
                          onClick={() => setEditingRemote(null)}
                        >
                          <X className="h-3.5 w-3.5" />
                        </Button>
                      </div>
                    );
                  }

                  return (
                    <div
                      key={remote.name}
                      className="group flex items-center gap-2 px-2 py-1 rounded-md hover:bg-muted/50"
                    >
                      <span className="font-mono font-semibold text-sm shrink-0 max-w-[100px] truncate">
                        {remote.name}
                      </span>
                      <Tooltip>
                        <TooltipTrigger asChild>
                          <span className="text-sm text-muted-foreground truncate flex-1 min-w-0 cursor-default">
                            {remote.url}
                          </span>
                        </TooltipTrigger>
                        <TooltipContent>
                          <p>{remote.url}</p>
                        </TooltipContent>
                      </Tooltip>
                      <Button
                        size="sm"
                        variant="ghost"
                        className="h-7 w-7 p-0 shrink-0 opacity-0 group-hover:opacity-100"
                        onClick={() => {
                          setEditingRemote(remote.name);
                          setEditUrl(remote.url);
                        }}
                      >
                        <Pencil className="h-3.5 w-3.5" />
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        className="h-7 w-7 p-0 shrink-0 opacity-0 group-hover:opacity-100 text-destructive hover:text-destructive"
                        onClick={() => setDeletingRemote(remote.name)}
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </Button>
                    </div>
                  );
                })}
              </div>
            )}

            <Separator />

            <div className="flex items-center gap-2">
              <Input
                placeholder="name"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                className="h-8 text-sm flex-[2] min-w-0"
                onKeyDown={(e) => {
                  if (e.key === "Enter" && canAdd) handleAdd();
                }}
              />
              <Input
                placeholder="https://github.com/..."
                value={newUrl}
                onChange={(e) => setNewUrl(e.target.value)}
                className="h-8 text-sm flex-[5] min-w-0"
                onKeyDown={(e) => {
                  if (e.key === "Enter" && canAdd) handleAdd();
                }}
              />
              <Button
                size="sm"
                className="h-8 shrink-0"
                disabled={!canAdd}
                onClick={handleAdd}
              >
                <Plus className="h-3.5 w-3.5 mr-1" /> Add
              </Button>
            </div>
          </div>
        </TooltipProvider>
      </DialogContent>
    </Dialog>
  );
}
