import { useEffect, useState, useCallback, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  listCollections,
  getCollection,
  onCollectionChanged,
  createCollection,
  saveRequest,
  createFolder,
  deleteCollection,
  deleteFolder,
  deleteRequest,
  moveItem,
  type CollectionSummary,
  type CollectionItem,
} from "@/lib/tauri-api";
import { usePaneStore } from "@/stores/pane-store";
import { useWorkspaceStore } from "@/stores/workspace-store";
import type { PaneNode } from "@/types/pane-types";
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
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";
import {
  FilePlus,
  Folder,
  LayoutDashboard,
  Search,
  Plus,
  Upload,
  Layers,
} from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { HistoryPanel } from "@/components/history/HistoryPanel";
import { Tree } from "@/components/ui/tree";
import { CollectionNode } from "@/components/collections/CollectionNode";
import type { DeleteTarget } from "@/components/collections/tree-utils";
import { WorkspaceSection } from "./WorkspaceSection";

// Sidebar panel with Collections tree and History tabs.
export function CollectionsSidebar() {
  const [summaries, setSummaries] = useState<CollectionSummary[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const filter = searchQuery.toLowerCase().trim();
  const [selectedId, setSelectedId] = useState<string>("");

  const [view, setView] = useState<"collections" | "history">("collections");

  const collectionsTabRef = useRef<HTMLButtonElement>(null);
  const historyTabRef = useRef<HTMLButtonElement>(null);

  const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const workspaces = useWorkspaceStore((s) => s.workspaces);
  const multiWorkspaceMode = useWorkspaceStore((s) => s.multiWorkspaceMode);
  const openWorkspaceTabs = usePaneStore((s) => s.openWorkspaceTabs);
  const openEphemeralTab = usePaneStore((s) => s.openEphemeralTab);
  const activeWorkspace = workspaces.find((w) => w.id === activeWorkspaceId);

  const handleImport = useCallback(async () => {
    const file = await open({
      multiple: false,
      filters: [{ name: "Collection", extensions: ["json"] }],
    });
    if (file) {
      console.log("Import file selected:", file);
    }
  }, []);

  const [isCreating, setIsCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [createError, setCreateError] = useState("");

  const [deleteTarget, setDeleteTarget] = useState<DeleteTarget | null>(null);

  const fetchCollections = useCallback(async () => {
    try {
      const results = await listCollections();
      setSummaries(results);
    } catch (err) {
      console.error("[CollectionsSidebar] list error", err);
    }
  }, []);

  const confirmDelete = useCallback(async () => {
    if (!deleteTarget) return;
    try {
      if (deleteTarget.type === "collection") {
        await deleteCollection(deleteTarget.collection);
      } else if (deleteTarget.type === "folder") {
        await deleteFolder(deleteTarget.collection, deleteTarget.path!);
      } else {
        await deleteRequest(deleteTarget.collection, deleteTarget.path!);
      }
      // Close open tabs for deleted items.
      const store = usePaneStore.getState();
      const closeTabs = (node: PaneNode): void => {
        if (node.type === "leaf") {
          for (const tab of node.tabs) {
            if (!tab.source) continue;
            const matches =
              (deleteTarget.type === "collection" &&
                tab.source.collection === deleteTarget.collection) ||
              (deleteTarget.type === "request" &&
                tab.source.collection === deleteTarget.collection &&
                tab.source.path === deleteTarget.path) ||
              (deleteTarget.type === "folder" &&
                tab.source.collection === deleteTarget.collection &&
                tab.source.path.startsWith(deleteTarget.path!));
            if (matches) store.closeTab(tab.id, node.groupId);
          }
        } else {
          closeTabs(node.children[0]);
          closeTabs(node.children[1]);
        }
      };
      closeTabs(store.root);
    } catch (err) {
      console.error("Delete failed:", err);
    }
    setDeleteTarget(null);
  }, [deleteTarget]);

  const INVALID_CHARS = /[/\\:*?"<>|]/;

  const handleCreateCollection = useCallback(async () => {
    const trimmed = newName.trim();
    if (!trimmed) {
      setIsCreating(false);
      setNewName("");
      return;
    }
    if (INVALID_CHARS.test(trimmed)) {
      setCreateError("Name contains invalid characters.");
      return;
    }
    try {
      await createCollection(trimmed);
      setIsCreating(false);
      setNewName("");
      setCreateError("");
    } catch (err) {
      setCreateError(
        err instanceof Error ? err.message : "Failed to create collection.",
      );
    }
  }, [newName]);

  const handleNewFolder = useCallback(
    async (collection: string, folderPath: string) => {
      // Find next available name (New Folder, New Folder 2, New Folder 3...).
      let name = "New Folder";
      try {
        const col = await getCollection(collection);
        const items = col.root.items;
        const existing = new Set(
          items
            .filter((i: CollectionItem) => i.type === "folder")
            .map((i: CollectionItem) => i.name),
        );
        let counter = 1;
        while (existing.has(name)) {
          counter++;
          name = `New Folder ${counter}`;
        }
      } catch {
        /* Use default name if fetch fails. */
      }
      const path = folderPath ? `${folderPath}/${name}` : name;
      try {
        await createFolder(collection, path);
      } catch (err) {
        console.error("[CollectionsSidebar] create folder failed:", err);
      }
    },
    [],
  );

  const handleMove = useCallback(
    async (
      srcCollection: string,
      srcPath: string,
      dstCollection: string,
      dstPath: string,
    ) => {
      await moveItem(srcCollection, srcPath, dstCollection, dstPath);
    },
    [],
  );

  const handleDuplicate = useCallback(
    async (collection: string, path: string, name: string) => {
      try {
        // Read the existing collection to locate the source request.
        const col = await getCollection(collection);
        const items = col.root.items;

        // Recursively find the request by fileName or name.
        const findRequest = (
          nodes: CollectionItem[],
          targetPath: string,
        ): CollectionItem | undefined => {
          for (const item of nodes) {
            if (item.type === "request") {
              const fn = item.fileName ?? item.name;
              if (fn === targetPath || item.name === name) return item;
            }
            if (item.type === "folder") {
              const found = findRequest(item.items, targetPath);
              if (found) return found;
            }
          }
          return undefined;
        };

        const source = findRequest(items, path.split("/").pop() ?? path);
        if (!source || source.type !== "request") return;

        // Collect existing names at the top level to find a unique copy name.
        const existing = new Set(
          items
            .filter((i: CollectionItem) => i.type === "request")
            .map((i: CollectionItem) => i.name),
        );
        let copyName = `${name} copy`;
        let counter = 1;
        while (existing.has(copyName)) {
          counter++;
          copyName = `${name} copy ${counter}`;
        }

        // Preserve the folder prefix from the original path.
        const pathParts = path.split("/");
        pathParts.pop();
        const folderPath = pathParts.join("/");
        const newPath = folderPath ? `${folderPath}/${copyName}` : copyName;

        // Save the duplicate with all source data. New uid and name, rest copied.
        const {
          type: _t,
          uid: _u,
          name: _n,
          fileName: _f,
          ...requestData
        } = source;
        await saveRequest(collection, newPath, {
          ...requestData,
          uid: "",
          name: copyName,
        });
      } catch (err) {
        console.error("[CollectionsSidebar] duplicate failed:", err);
      }
    },
    [],
  );

  // Load collections on mount. Debounce file watcher events so rapid
  // filesystem changes collapse into one refresh.
  const listDebounce = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    void fetchCollections();
    let cancelled = false;
    const unlisteners: Array<() => void> = [];

    onCollectionChanged(() => {
      if (listDebounce.current) clearTimeout(listDebounce.current);
      listDebounce.current = setTimeout(() => void fetchCollections(), 300);
    }).then((fn) => {
      if (cancelled) fn();
      else unlisteners.push(fn);
    });

    // Reload when the user switches workspaces — the backend now reads from
    // the new workspace path, so we just need to trigger a fresh fetch.
    listen("workspace-switched", () => {
      void fetchCollections();
    }).then((fn) => {
      if (cancelled) fn();
      else unlisteners.push(fn);
    });

    // Reload when git operations change the branch (files on disk change).
    listen("git-changed", () => {
      if (listDebounce.current) clearTimeout(listDebounce.current);
      listDebounce.current = setTimeout(() => void fetchCollections(), 300);
    }).then((fn) => {
      if (cancelled) fn();
      else unlisteners.push(fn);
    });

    return () => {
      cancelled = true;
      if (listDebounce.current) clearTimeout(listDebounce.current);
      unlisteners.forEach((fn) => fn());
    };
  }, [fetchCollections]);

  return (
    <div className="h-full flex flex-col bg-card/50 backdrop-blur-sm border-r border-border-70">
      <div className="flex-1 flex flex-col overflow-hidden">
        {/* View tabs and action icons (Bruno-style). */}
        <div className="flex items-center justify-between px-3 pt-2.5 pb-1">
          <div
            role="tablist"
            aria-label="Sidebar views"
            aria-orientation="horizontal"
            className="flex items-center gap-0.5"
            onKeyDown={(e) => {
              if (e.key === "ArrowRight") {
                e.preventDefault();
                setView("history");
                historyTabRef.current?.focus();
              } else if (e.key === "ArrowLeft") {
                e.preventDefault();
                setView("collections");
                collectionsTabRef.current?.focus();
              } else if (e.key === "Home") {
                e.preventDefault();
                setView("collections");
                collectionsTabRef.current?.focus();
              } else if (e.key === "End") {
                e.preventDefault();
                setView("history");
                historyTabRef.current?.focus();
              }
            }}
          >
            <button
              ref={collectionsTabRef}
              type="button"
              role="tab"
              id="tab-collections"
              aria-selected={view === "collections"}
              aria-controls="panel-collections"
              tabIndex={view === "collections" ? 0 : -1}
              onClick={() => setView("collections")}
              className={cn(
                "px-2 py-1 text-xs font-medium rounded-md transition-colors",
                view === "collections"
                  ? "bg-accent text-accent-foreground"
                  : "text-muted-foreground hover:text-foreground hover:bg-accent/50",
              )}
            >
              {multiWorkspaceMode ? "Workspaces" : "Collections"}
            </button>
            <button
              ref={historyTabRef}
              type="button"
              role="tab"
              id="tab-history"
              aria-selected={view === "history"}
              aria-controls="panel-history"
              tabIndex={view === "history" ? 0 : -1}
              onClick={() => setView("history")}
              className={cn(
                "px-2 py-1 text-xs font-medium rounded-md transition-colors",
                view === "history"
                  ? "bg-accent text-accent-foreground"
                  : "text-muted-foreground hover:text-foreground hover:bg-accent/50",
              )}
            >
              History
            </button>
          </div>
          {view === "collections" && (
            <div className="flex items-center gap-0.5">
              <Button
                variant="ghost"
                size="icon"
                className="h-5 w-5 shrink-0 text-muted-foreground hover:text-foreground"
                onClick={() => setIsCreating(true)}
                aria-label="New Collection"
                title="New Collection"
              >
                <Plus className="h-3.5 w-3.5" />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                className="h-5 w-5 shrink-0 text-muted-foreground hover:text-foreground"
                onClick={() => void handleImport()}
                aria-label="Import Collection"
                title="Import Collection"
              >
                <Upload className="h-3.5 w-3.5" />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                className="h-5 w-5 shrink-0 text-muted-foreground hover:text-foreground"
                onClick={() => openEphemeralTab('http')}
                aria-label="New unsaved request"
                title="New unsaved request"
              >
                <FilePlus className="h-3.5 w-3.5" />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                className="h-5 w-5 shrink-0 text-muted-foreground hover:text-foreground"
                aria-label={
                  multiWorkspaceMode
                    ? "Switch to single workspace mode"
                    : "Switch to multi-workspace mode"
                }
                title={
                  multiWorkspaceMode
                    ? "Switch to single workspace mode"
                    : "Switch to multi-workspace mode"
                }
                onClick={() => {
                  const store = useWorkspaceStore.getState();
                  void store.setMultiWorkspaceMode(!store.multiWorkspaceMode);
                }}
              >
                <Layers className="h-3.5 w-3.5" />
              </Button>
            </div>
          )}
        </div>

        {/* Collections panel — always mounted so aria-controls resolves. */}
        <div
          id="panel-collections"
          role="tabpanel"
          aria-labelledby="tab-collections"
          tabIndex={0}
          className={cn(
            "flex-1 flex flex-col overflow-hidden",
            view !== "collections" && "hidden",
          )}
        >
          {/* Workspace home button. */}
          {activeWorkspace && (
            <div className="px-2 pt-1">
              <Button
                variant="ghost"
                className="w-full justify-start h-8 px-2 text-sm font-medium"
                onClick={() =>
                  openWorkspaceTabs(activeWorkspace.id)
                }
              >
                <LayoutDashboard className="h-3.5 w-3.5 shrink-0 text-muted-foreground mr-2" />
                <span className="truncate">{activeWorkspace.name}</span>
              </Button>
            </div>
          )}
          {/* Search and inline create. */}
          <div className="px-2 pb-2 space-y-1.5">
            <div className="relative">
              <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                className="h-8 pl-7 text-sm"
                placeholder="Search requests..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                aria-label="Search collections"
              />
            </div>
            {isCreating && (
              <div className="px-1">
                <Input
                  autoFocus
                  className="h-8 text-sm"
                  placeholder="Collection name"
                  value={newName}
                  onChange={(e) => {
                    setNewName(e.target.value);
                    setCreateError("");
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") handleCreateCollection();
                    if (e.key === "Escape") {
                      setIsCreating(false);
                      setNewName("");
                      setCreateError("");
                    }
                  }}
                  onBlur={() => {
                    setIsCreating(false);
                    setNewName("");
                    setCreateError("");
                  }}
                />
                {createError && (
                  <p className="text-xs text-destructive mt-0.5 px-1">
                    {createError}
                  </p>
                )}
              </div>
            )}
          </div>

          {/* Collection tree. */}
          {multiWorkspaceMode ? (
            <ScrollArea className="flex-1">
              <div className="px-1 pb-2 space-y-1">
                {workspaces.map((ws) => (
                  <WorkspaceSection
                    key={ws.id}
                    workspace={ws}
                    collectionCount={
                      ws.id === activeWorkspaceId ? summaries.length : 0
                    }
                  >
                    {ws.id === activeWorkspaceId ? (
                      summaries.map((s) => (
                        <CollectionNode
                          key={s.name}
                          summary={s}
                          filter={filter}
                          summaries={summaries}
                          onNewFolder={handleNewFolder}
                          onMove={handleMove}
                          onDelete={setDeleteTarget}
                          onDuplicate={handleDuplicate}
                        />
                      ))
                    ) : (
                      <div className="px-4 py-2 text-xs text-muted-foreground">
                        Switch to this workspace to see collections
                      </div>
                    )}
                  </WorkspaceSection>
                ))}
              </div>
            </ScrollArea>
          ) : (
            <ScrollArea className="flex-1">
              <Tree value={selectedId} onValueChange={setSelectedId}>
                <div className="px-1 pb-2">
                  {summaries.length === 0 ? (
                    <div className="flex flex-col items-center justify-center py-8 px-4">
                      <Folder className="h-8 w-8 text-muted-foreground/50 mb-2" />
                      <p className="text-sm text-muted-foreground mb-3">
                        No collections yet.
                      </p>
                      <Button
                        variant="outline"
                        size="sm"
                        className="text-sm"
                        onClick={() => setIsCreating(true)}
                      >
                        <Plus className="h-3.5 w-3.5 mr-1.5" />
                        Create Collection
                      </Button>
                    </div>
                  ) : (
                    summaries.map((s) => (
                      <CollectionNode
                        key={s.name}
                        summary={s}
                        filter={filter}
                        summaries={summaries}
                        onNewFolder={handleNewFolder}
                        onMove={handleMove}
                        onDelete={setDeleteTarget}
                        onDuplicate={handleDuplicate}
                      />
                    ))
                  )}
                </div>
              </Tree>
            </ScrollArea>
          )}
        </div>

        {/* History panel — always mounted so aria-controls resolves. */}
        <div
          id="panel-history"
          role="tabpanel"
          aria-labelledby="tab-history"
          tabIndex={0}
          className={cn(
            "flex-1 overflow-hidden",
            view !== "history" && "hidden",
          )}
        >
          <HistoryPanel />
        </div>
      </div>

      <AlertDialog
        open={!!deleteTarget}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null);
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Confirm Delete</AlertDialogTitle>
            <AlertDialogDescription>
              {deleteTarget?.type === "collection"
                ? `Delete collection '${deleteTarget.name}'? This removes all requests inside it.`
                : deleteTarget?.type === "folder"
                  ? `Delete folder '${deleteTarget.name}' and all requests inside it?`
                  : `Delete request '${deleteTarget?.name}'?`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={() => void confirmDelete()}>
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
