import { useState, useEffect, useCallback, useRef } from "react";
import {
  Library,
  LibraryBig,
  FolderPlus,
  Plus,
  Trash2,
  MoreHorizontal,
  Package,
  PackageOpenIcon,
  ArrowBigDown,
  ArrowDown,
  ArrowRight,
  ChevronDownCircle,
  ChevronDown,
  ChevronRight,
} from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { Input } from "@/components/ui/input";
import { TreeItem, TreeItemContent } from "@/components/ui/tree";
import {
  getCollection,
  onCollectionChanged,
  renameCollection,
  saveRequest,
} from "@/lib/tauri-api";
import { usePaneStore } from "@/stores/pane-store";
import { createDefaultRequest } from "@/lib/pane-utils";
import { FolderNode } from "./FolderNode";
import { RequestNode } from "./RequestNode";
import type { CollectionSummary, Collection } from "@/lib/tauri-api";
import type { CollectionTab } from "@/types/pane-types";
import type { DeleteTarget } from "./tree-utils";

interface CollectionNodeProps {
  summary: CollectionSummary;
  filter: string;
  summaries: CollectionSummary[];
  onNewFolder: (collection: string, folderPath: string) => Promise<void>;
  onMove: (
    srcCollection: string,
    srcPath: string,
    dstCollection: string,
    dstPath: string,
  ) => Promise<void>;
  onDelete: (target: DeleteTarget) => void;
  onDuplicate: (
    collection: string,
    path: string,
    name: string,
  ) => Promise<void>;
}

export function CollectionNode({
  summary,
  filter,
  summaries,
  onNewFolder,
  onMove,
  onDelete,
  onDuplicate,
}: CollectionNodeProps) {
  const [open, setOpen] = useState(false);
  const [collection, setCollection] = useState<Collection | null>(null);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState(summary.name);
  const clickTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const treeDebounce = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [creatingRequest, setCreatingRequest] = useState(false);
  const [newRequestName, setNewRequestName] = useState("");

  const refreshTree = useCallback(() => {
    getCollection(summary.name)
      .then(setCollection)
      .catch((err) => console.error("[CollectionNode] fetch error", err));
  }, [summary.name]);

  // Fetch when first expanded.
  useEffect(() => {
    if (open && !collection) refreshTree();
  }, [open, collection, refreshTree]);

  // Per-collection change listener, active only when expanded.
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    onCollectionChanged((event) => {
      const affected = event.collection ?? event.name;
      if (!affected || affected === summary.name) {
        if (treeDebounce.current) clearTimeout(treeDebounce.current);
        treeDebounce.current = setTimeout(() => refreshTree(), 300);
      }
    }).then((fn) => {
      if (cancelled)
        fn(); // Already unmounted — immediately unsubscribe.
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
      if (treeDebounce.current) clearTimeout(treeDebounce.current);
    };
  }, [open, refreshTree, summary.name]);

  // Auto-expand when filter is active.
  useEffect(() => {
    if (filter) setOpen(true);
  }, [filter]);

  // Clear pending click timer on unmount.
  useEffect(() => {
    return () => {
      if (clickTimer.current) clearTimeout(clickTimer.current);
    };
  }, []);

  const handleRename = async () => {
    const trimmed = renameValue.trim();
    if (!trimmed || trimmed === summary.name) {
      setIsRenaming(false);
      return;
    }
    try {
      await renameCollection(summary.name, trimmed);
      setIsRenaming(false);
    } catch (err) {
      console.error("Rename collection failed:", err);
    }
  };

  // Single click toggles expand after 250 ms (to allow double-click to fire first).
  const handleClick = () => {
    if (clickTimer.current) {
      clearTimeout(clickTimer.current);
      clickTimer.current = null;
      return;
    }
    clickTimer.current = setTimeout(() => {
      clickTimer.current = null;
      setOpen((prev) => !prev);
    }, 250);
  };

  // Double click opens the collection Overview tab.
  const handleDoubleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (clickTimer.current) {
      clearTimeout(clickTimer.current);
      clickTimer.current = null;
    }
    const tab: CollectionTab = {
      id: summary.uid,
      title: summary.name,
      tabType: "collection",
      collectionName: summary.name,
      isDirty: false,
    };
    usePaneStore.getState().openTab(tab);
  };

  const handleNewRequestCreate = async () => {
    const name = newRequestName.trim();
    if (!name) {
      setCreatingRequest(false);
      return;
    }
    setCreatingRequest(false);
    try {
      const uid = crypto.randomUUID();
      const payload = {
        uid,
        name,
        method: "GET" as const,
        url: "",
        headers: [],
        auth: { authType: "none" as const },
      };
      const saved = await saveRequest(summary.name, name, payload);
      usePaneStore.getState().openTab({
        id: uid,
        title: saved.name,
        tabType: "request",
        request: createDefaultRequest(),
        response: null,
        isDirty: false,
        source: {
          collection: summary.name,
          path: saved.fileName ?? `${name}.yml`,
        },
      });
    } catch (err) {
      console.error("[CollectionNode] Failed to create request:", err);
    }
  };

  const rawItems = collection?.root.items ?? [];
  const filteredItems = filter
    ? rawItems.filter(
        (item) =>
          item.type !== "request" ||
          item.name.toLowerCase().includes(filter.toLowerCase()),
      )
    : rawItems;

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div className="group relative flex items-center">
          <TreeItem
            value={summary.uid}
            open={open}
            onOpenChange={setOpen}
            className="flex-1"
          >
            <TreeItemContent
              className="flex gap-3 w-full px-2 py-0.5 cursor-pointer"
              onClick={handleClick}
              onDoubleClick={handleDoubleClick}
              aria-label={`${open ? "Collapse" : "Expand"} collection ${summary.name}`}
            >
              {open ? (
                <ChevronDown
                  className="h-4 w-4 flex-none text-primary-foreground"
                  strokeWidth={1.5}
                />
              ) : (
                <ChevronRight
                  className="h-4 w-4 flex-none text-primary-foreground"
                  strokeWidth={1.5}
                />
              )}
              {isRenaming ? (
                <Input
                  autoFocus
                  className="h-6 text-sm flex-1"
                  value={renameValue}
                  onChange={(e) => setRenameValue(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") void handleRename();
                    if (e.key === "Escape") setIsRenaming(false);
                  }}
                  onBlur={() => void handleRename()}
                  onClick={(e) => e.stopPropagation()}
                />
              ) : (
                <>
                  <span className="truncate font-medium text-foreground">
                    {summary.name}
                  </span>
                  {summary.refType === 'external' && (
                    <span className="ml-auto shrink-0 text-2xs text-muted-foreground bg-muted px-1.5 py-0.5 rounded">
                      ext
                    </span>
                  )}
                </>
              )}
            </TreeItemContent>
          </TreeItem>

          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                className="absolute right-1 h-5 w-5 flex items-center justify-center rounded-sm opacity-0 group-hover:opacity-100 hover:bg-muted text-muted-foreground"
                onClick={(e) => e.stopPropagation()}
              >
                <MoreHorizontal className="h-3 w-3" />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent
              className="w-48"
              onClick={(e) => e.stopPropagation()}
            >
              <DropdownMenuItem
                onClick={(e) =>
                  handleDoubleClick(e as unknown as React.MouseEvent)
                }
              >
                Overview
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                onClick={() => {
                  setOpen(true);
                  setCreatingRequest(true);
                  setNewRequestName("");
                }}
              >
                <Plus className="h-3.5 w-3.5 mr-2" /> New Request
              </DropdownMenuItem>
              <DropdownMenuItem
                onClick={async () => {
                  await onNewFolder(summary.name, "");
                  setOpen(true);
                }}
              >
                <FolderPlus className="h-3.5 w-3.5 mr-2" /> New Folder
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                onClick={() => {
                  setRenameValue(summary.name);
                  setIsRenaming(true);
                }}
              >
                Rename
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                className="text-destructive"
                onClick={() =>
                  onDelete({
                    type: "collection",
                    collection: summary.name,
                    name: summary.name,
                  })
                }
              >
                <Trash2 className="h-3.5 w-3.5 mr-2" /> Delete
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </ContextMenuTrigger>

      <ContextMenuContent className="w-48">
        <ContextMenuItem
          onClick={() => {
            const tab: CollectionTab = {
              id: summary.uid,
              title: summary.name,
              tabType: "collection",
              collectionName: summary.name,
              isDirty: false,
            };
            usePaneStore.getState().openTab(tab);
          }}
        >
          Overview
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem
          onClick={() => {
            setOpen(true);
            setCreatingRequest(true);
            setNewRequestName("");
          }}
        >
          New Request
        </ContextMenuItem>
        <ContextMenuItem onClick={() => void onNewFolder(summary.name, "")}>
          New Folder
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem
          onClick={() => {
            setRenameValue(summary.name);
            setIsRenaming(true);
          }}
        >
          Rename
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem
          className="text-destructive"
          onClick={() =>
            onDelete({
              type: "collection",
              collection: summary.name,
              name: summary.name,
            })
          }
        >
          Delete
        </ContextMenuItem>
      </ContextMenuContent>

      {open && collection && (
        <div className="pl-1.5 border-l border-border/30 ml-2">
          {filteredItems.map((item) => {
            if (item.type === "folder") {
              return (
                <FolderNode
                  key={`folder-${item.name}`}
                  name={item.name}
                  items={item.items}
                  collectionName={summary.name}
                  basePath={item.name}
                  depth={1}
                  filter={filter}
                  summaries={summaries}
                  onNewFolder={onNewFolder}
                  onMove={onMove}
                  onDelete={onDelete}
                  onDuplicate={onDuplicate}
                />
              );
            }
            return (
              <RequestNode
                key={item.uid}
                uid={item.uid}
                name={item.name}
                method={item.method}
                collectionName={summary.name}
                path={item.fileName ?? item.name}
                itemData={item}
                summaries={summaries}
                onMove={onMove}
                onDelete={onDelete}
                onDuplicate={onDuplicate}
              />
            );
          })}
          {creatingRequest && (
            <div className="flex items-center gap-1 px-2 py-0.5 text-sm">
              <Input
                autoFocus
                className="h-5 text-sm flex-1"
                placeholder="Request name"
                value={newRequestName}
                onChange={(e) => setNewRequestName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void handleNewRequestCreate();
                  if (e.key === "Escape") setCreatingRequest(false);
                }}
                onBlur={() => setCreatingRequest(false)}
                onClick={(e) => e.stopPropagation()}
              />
            </div>
          )}
        </div>
      )}
    </ContextMenu>
  );
}
