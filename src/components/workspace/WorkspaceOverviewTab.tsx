import { useState, useEffect, useCallback } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  Plus,
  FolderOpen,
  Layers,
  MoreHorizontal,
  Trash2,
  ExternalLink,
  FileText,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardHeader, CardContent } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  listCollections,
  linkExternalCollection,
  openFolderPicker,
  createCollection,
  deleteCollection,
  type CollectionSummary,
} from "@/lib/tauri-api";
import { useWorkspaceStore } from "@/stores/workspace-store";
import { usePaneStore } from "@/stores/pane-store";
import { useEnvStore } from "@/stores/env-store";
import type { CollectionTab } from "@/types/pane-types";

interface WorkspaceOverviewTabProps {
  workspaceId: string;
}

export function WorkspaceOverviewTab({
  workspaceId,
}: WorkspaceOverviewTabProps) {
  const workspace = useWorkspaceStore((s) =>
    s.workspaces.find((w) => w.id === workspaceId),
  );
  const updateDescription = useWorkspaceStore((s) => s.updateDescription);
  const openTab = usePaneStore((s) => s.openTab);
  const globalEnvironments = useEnvStore((s) => s.globalEnvironments);
  const loadGlobalEnvironments = useEnvStore((s) => s.loadGlobalEnvironments);

  const [summaries, setSummaries] = useState<CollectionSummary[]>([]);
  const [isCreating, setIsCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [docMode, setDocMode] = useState<"edit" | "preview">("preview");
  const [docContent, setDocContent] = useState<string>(
    workspace?.description ?? "",
  );

  const refresh = useCallback(async () => {
    const cols = await listCollections();
    setSummaries(cols);
  }, []);

  useEffect(() => {
    refresh().catch(console.error);
    loadGlobalEnvironments().catch(console.error);
  }, [refresh, loadGlobalEnvironments]);

  useEffect(() => {
    setDocContent(workspace?.description ?? "");
  }, [workspace?.description]);

  function handleOpenCollection(collectionName: string) {
    const tab: CollectionTab = {
      id: `collection-${collectionName}`,
      title: collectionName,
      tabType: "collection",
      collectionName,
      isDirty: false,
    };
    openTab(tab);
  }

  async function handleCreateCollection() {
    const name = newName.trim();
    if (!name) return;
    try {
      await createCollection(name);
      setNewName("");
      setIsCreating(false);
      await refresh();
    } catch (err) {
      console.error("[WorkspaceOverview] create failed:", err);
    }
  }

  async function handleDeleteCollection(name: string) {
    try {
      await deleteCollection(name);
      await refresh();
    } catch (err) {
      console.error("[WorkspaceOverview] delete failed:", err);
    }
  }

  async function handleSaveDoc() {
    try {
      await updateDescription(workspaceId, docContent.trim() || null);
    } catch (err) {
      console.error("[WorkspaceOverview] save doc failed:", err);
    }
  }

  async function handleLinkExternal() {
    try {
      const path = await openFolderPicker();
      if (path) {
        await linkExternalCollection(workspaceId, path);
        await refresh();
      }
    } catch (err) {
      console.error("[WorkspaceOverview] link external failed:", err);
    }
  }

  const collectionCount = summaries.length;

  return (
    <div className="flex h-full overflow-hidden">
      {/* ── LEFT COLUMN ── */}
      <div className="flex-1 border-r border-border overflow-hidden flex flex-col">
        <ScrollArea className="flex-1">
          <div className="p-5 flex flex-col gap-5">
            {/* Page header */}
            <h2 className="text-base font-semibold leading-tight">
              {workspace?.name ?? "Workspace"}
            </h2>

            {/* Stats — plain Bruno style */}
            <div className="flex gap-7 pb-4 border-b border-border">
              <div className="flex flex-col gap-0.5">
                <span className="text-[22px] font-bold leading-tight tabular-nums">
                  {collectionCount}
                </span>
                <span className="text-[11px] text-muted-foreground">
                  Collections
                </span>
              </div>
              <div className="flex flex-col gap-0.5">
                <span className="text-[22px] font-bold leading-tight tabular-nums">
                  {globalEnvironments.length}
                </span>
                <span className="text-[11px] text-muted-foreground">
                  Environments
                </span>
              </div>
            </div>

            {/* Quick Actions */}
            <div>
              <p className="text-[11px] font-medium text-muted-foreground mb-2">
                Quick Actions
              </p>
              {isCreating ? (
                <div className="flex gap-2">
                  <input
                    autoFocus
                    placeholder="Collection name"
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void handleCreateCollection();
                      if (e.key === "Escape") {
                        setIsCreating(false);
                        setNewName("");
                      }
                    }}
                    className="flex-1 rounded-md border border-input bg-transparent px-3 py-1.5 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                  />
                  <Button
                    size="sm"
                    onClick={() => void handleCreateCollection()}
                    disabled={!newName.trim()}
                  >
                    Create
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => {
                      setIsCreating(false);
                      setNewName("");
                    }}
                  >
                    Cancel
                  </Button>
                </div>
              ) : (
                <div className="flex flex-wrap gap-1.5">
                  <Button
                    variant="outline"
                    size="sm"
                    className="text-xs h-7"
                    onClick={() => setIsCreating(true)}
                  >
                    <Plus className="h-3 w-3 mr-1.5" />
                    Create Collection
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    className="text-xs h-7"
                    onClick={handleLinkExternal}
                  >
                    <FolderOpen className="h-3 w-3 mr-1.5" />
                    Open Collection
                  </Button>
                </div>
              )}
            </div>

            {/* Collections */}
            <div>
              <p className="text-[11px] font-medium text-muted-foreground mb-2">
                Collections
              </p>
              {summaries.length > 0 ? (
                <>
                  {summaries.map((col) => (
                    <Card
                      key={col.name}
                      className="mb-2 last:mb-0 bg-card/20 border rounded-md cursor-pointer hover:bg-card transition-colors"
                    >
                      <CardContent className="p-0">
                        <div
                          key={col.name}
                          role="button"
                          tabIndex={0}
                          onClick={() => handleOpenCollection(col.name)}
                          onKeyDown={(e) => {
                            if (e.key === "Enter" || e.key === " ") {
                              e.preventDefault();
                              handleOpenCollection(col.name);
                            }
                          }}
                          className="group flex items-center gap-2.5 px-3 py-2.5 border-b border-border last:border-b-0 hover:bg-muted/50 cursor-pointer transition-colors"
                        >
                          <div
                            className={`w-7 h-7 rounded-md flex items-center justify-center shrink-0 ${col.refType === "external" ? "bg-orange-950/40 border border-orange-900/40" : "bg-primary/10 border border-primary/20"}`}
                          >
                            <Layers className="h-3.5 w-3.5 text-primary" />
                          </div>
                          <div className="flex-1 min-w-0">
                            <span className="text-sm font-medium truncate block">
                              {col.name}
                            </span>
                            {col.path && (
                              <span className="text-[10px] text-muted-foreground truncate block">
                                {col.path}
                              </span>
                            )}
                          </div>
                          <span className="text-[10px] text-muted-foreground shrink-0">
                            {col.requestCount} req
                          </span>
                          {col.refType === "external" && (
                            <span className="shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400">
                              external
                            </span>
                          )}
                          <DropdownMenu>
                            <DropdownMenuTrigger asChild>
                              <button
                                type="button"
                                aria-label="Collection options"
                                className="h-5 w-5 flex items-center justify-center rounded-sm opacity-0 group-hover:opacity-100 focus-visible:opacity-100 hover:bg-muted text-muted-foreground"
                                onClick={(e) => e.stopPropagation()}
                              >
                                <MoreHorizontal className="h-3 w-3" />
                              </button>
                            </DropdownMenuTrigger>
                            <DropdownMenuContent
                              onClick={(e) => e.stopPropagation()}
                            >
                              <DropdownMenuItem
                                onClick={() => handleOpenCollection(col.name)}
                              >
                                <ExternalLink className="h-3.5 w-3.5 mr-2" />{" "}
                                Open
                              </DropdownMenuItem>
                              <DropdownMenuSeparator />
                              <DropdownMenuItem
                                className="text-destructive"
                                onClick={() => handleDeleteCollection(col.name)}
                              >
                                <Trash2 className="h-3.5 w-3.5 mr-2" /> Delete
                              </DropdownMenuItem>
                            </DropdownMenuContent>
                          </DropdownMenu>
                        </div>
                      </CardContent>
                    </Card>
                  ))}
                </>
              ) : (
                <p className="text-sm text-muted-foreground">
                  No collections yet.
                </p>
              )}
            </div>
          </div>
        </ScrollArea>
      </div>

      {/* ── RIGHT COLUMN — Documentation ── */}
      <div className="flex-1 p-4 flex flex-col overflow-hidden">
        <Card className="flex-1 flex flex-col overflow-hidden">
          <CardHeader className="flex flex-row items-center justify-between py-2.5 px-4 shrink-0">
            <div className="flex items-center gap-2">
              <FileText className="h-3.5 w-3.5 text-muted-foreground" />
              <span className="text-xs font-semibold text-muted-foreground">
                Documentation
              </span>
            </div>
            <Tabs
              value={docMode}
              onValueChange={(v) => setDocMode(v as "edit" | "preview")}
            >
              <TabsList className="h-6">
                <TabsTrigger value="edit" className="text-[10px] px-2.5 py-0.5">
                  Edit
                </TabsTrigger>
                <TabsTrigger
                  value="preview"
                  className="text-[10px] px-2.5 py-0.5"
                >
                  Preview
                </TabsTrigger>
              </TabsList>
            </Tabs>
          </CardHeader>

          <CardContent className="flex-1 p-0 overflow-hidden flex flex-col">
            {/* Edit pane */}
            {docMode === "edit" && (
              <div className="flex-1 flex flex-col overflow-hidden">
                <textarea
                  className="flex-1 w-full bg-transparent border-none resize-none px-4 py-3.5 text-xs font-mono text-muted-foreground placeholder:text-muted-foreground/40 focus-visible:outline-none leading-relaxed"
                  placeholder={"Add documentation...\n\nSupports **Markdown**"}
                  value={docContent}
                  onChange={(e) => setDocContent(e.target.value)}
                  onBlur={() => void handleSaveDoc()}
                />
                <div className="flex justify-end items-center gap-2 px-3 py-2 border-t border-border shrink-0">
                  <span className="text-[10px] text-muted-foreground/50">
                    Markdown supported · saves on blur
                  </span>
                  <Button
                    size="sm"
                    className="h-6 text-[10px] px-3"
                    onClick={() => void handleSaveDoc()}
                  >
                    Save
                  </Button>
                </div>
              </div>
            )}

            {/* Preview pane */}
            {docMode === "preview" && (
              <div className="flex-1 overflow-y-auto px-4 py-3.5">
                {docContent.trim() ? (
                  <div className="prose-doc text-xs leading-relaxed">
                    <ReactMarkdown remarkPlugins={[remarkGfm]}>
                      {docContent}
                    </ReactMarkdown>
                  </div>
                ) : (
                  <div className="h-full flex flex-col items-center justify-center gap-3 text-center py-8">
                    <FileText className="h-9 w-9 text-muted-foreground/20" />
                    <div className="space-y-1">
                      <p className="text-xs font-medium text-muted-foreground/60">
                        Add documentation to help your team work smoothly.
                      </p>
                      <p className="text-[11px] text-muted-foreground/40">
                        You can include project overview, setup instructions,
                        key workflows, and FAQs.
                      </p>
                    </div>
                    <Button
                      variant="outline"
                      size="sm"
                      className="text-xs h-7 mt-1"
                      onClick={() => setDocMode("edit")}
                    >
                      + Add Documentation
                    </Button>
                  </div>
                )}
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
