# Git Merge Conflict Resolution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When a pull results in conflicts, surface a conflict editor inside the Git panel right pane and leave the repo in merge-in-progress state so the user can resolve each file and commit normally.

**Architecture:** Three isolated file changes. The backend returns `Ok(())` on conflicts instead of an error. The frontend detects the conflicted state via the existing status refresh, shows a banner in the left panel, routes conflicted file clicks to the existing `ConflictResolver` component in the right panel.

**Tech Stack:** Rust (git2 crate), React, TypeScript, Tailwind CSS, Zustand, Lucide icons, Shadcn UI

---

## File Map

| File | Change |
|------|--------|
| `crates/rocket-git/src/git2_service.rs` | In `pull`: write index and return `Ok(())` on conflicts instead of propagating an error |
| `src/components/git/GitFileList.tsx` | Add `onConflictClick` prop; render `AlertTriangle` for conflicted files; async load + route on click; suppress action buttons for conflicted files |
| `src/components/git/GitPanel.tsx` | Add `conflict` variant to `RightPanelView`; render `ConflictResolver`; add in-merge banner; add breadcrumb label; wire `onConflictClick` |

---

## Task 1: Backend — Pull Returns Ok(()) on Conflicts

**Files:**
- Modify: `crates/rocket-git/src/git2_service.rs:544-555`

- [ ] **Step 1: Apply the change**

In `crates/rocket-git/src/git2_service.rs`, find the block after the normal merge (lines 544–555):

```rust
        // Normal merge.
        repo.merge(&[&fetch_commit], None, None)
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let mut index = repo
            .index()
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        if index.has_conflicts() {
            return Err(DomainError::Internal(
                "pull resulted in conflicts".to_string(),
            ));
        }
```

Replace with:

```rust
        // Normal merge.
        repo.merge(&[&fetch_commit], None, None)
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let mut index = repo
            .index()
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        if index.has_conflicts() {
            // Leave the repo in merge-in-progress state. The frontend detects
            // conflicts via the next status refresh.
            index
                .write()
                .map_err(|e| DomainError::Internal(e.to_string()))?;
            return Ok(());
        }
```

- [ ] **Step 2: Verify compilation**

```bash
cargo check -p rocket-git
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-git/src/git2_service.rs
git commit -m "fix: pull leaves repo in merge-in-progress state on conflicts"
```

---

## Task 2: GitFileList — Conflicted File Routing

**Files:**
- Modify: `src/components/git/GitFileList.tsx`

- [ ] **Step 1: Apply the change**

Replace the entire file contents with:

```tsx
import { Plus, Minus, Trash2, AlertTriangle } from "lucide-react";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
  TooltipProvider,
} from "@/components/ui/tooltip";
import { useGitStore } from "@/stores/git-store";
import { GIT_STATUS_CONFIG } from "@/lib/colors";
import type { ConflictFile, FileStatus } from "@/lib/tauri-api";

interface GitFileListProps {
  onFileClick: (file: FileStatus) => void;
  onConflictClick: (conflictFile: ConflictFile) => void;
}

export function GitFileList({ onFileClick, onConflictClick }: GitFileListProps) {
  const {
    status,
    conflicts,
    refreshConflicts,
    refreshStatus,
    stageFiles,
    stageAll,
    unstageFiles,
    unstageAll,
    discardFiles,
  } = useGitStore();

  const staged = status?.files.filter((f) => f.staged) ?? [];
  const unstaged =
    status?.files.filter((f) => !f.staged && f.status !== "unchanged") ?? [];

  const handleDiscardAll = (e: React.MouseEvent) => {
    e.stopPropagation();
    discardFiles(unstaged.filter((f) => f.status !== "conflicted").map((f) => f.path));
  };

  const handleStageAll = (e: React.MouseEvent) => {
    e.stopPropagation();
    stageAll();
  };

  const handleUnstageAll = (e: React.MouseEvent) => {
    e.stopPropagation();
    unstageAll();
  };

  const handleConflictClick = async (file: FileStatus) => {
    await refreshConflicts();
    const conflictFile = conflicts.find((c) => c.path === file.path);
    if (conflictFile) {
      onConflictClick(conflictFile);
    } else {
      await refreshStatus();
    }
  };

  return (
    <TooltipProvider>
      <ScrollArea className="flex-1">
        <div className="p-3 space-y-1">
          {/* Staged section — shown above unstaged, only when files are staged. */}
          {staged.length > 0 && (
            <>
              <div className="flex items-center justify-between px-2 py-1">
                <span className="text-xs font-medium text-muted-foreground">
                  Staged Changes
                </span>
                <div className="flex items-center gap-1.5">
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-5 w-5"
                        onClick={handleUnstageAll}
                      >
                        <Minus className="h-3.5 w-3.5" />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>Unstage all</TooltipContent>
                  </Tooltip>
                  <span className="text-xs text-muted-foreground">
                    {staged.length}
                  </span>
                </div>
              </div>

              {/* Staged file rows. */}
              {staged.map((file) => (
                <div
                  key={file.path}
                  className="group flex items-center px-2 py-1 rounded-md hover:bg-muted/50 cursor-pointer gap-1.5"
                  onClick={() => onFileClick(file)}
                >
                  <span className="text-sm truncate flex-1 min-w-0">
                    {file.path}
                  </span>
                  <span
                    className={`text-xs font-medium shrink-0 ${GIT_STATUS_CONFIG[file.status].className}`}
                  >
                    {GIT_STATUS_CONFIG[file.status].label}
                  </span>
                  <div className="hidden gap-0.5 shrink-0 group-hover:flex">
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-5 w-5"
                          onClick={(e) => {
                            e.stopPropagation();
                            unstageFiles([file.path]);
                          }}
                        >
                          <Minus className="h-3.5 w-3.5" />
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent>Unstage</TooltipContent>
                    </Tooltip>
                  </div>
                </div>
              ))}
              <Separator className="my-2" />
            </>
          )}

          {/* Unstaged section header. */}
          <div className="flex items-center justify-between px-2 py-1">
            <span className="text-xs font-medium text-muted-foreground">
              Unstaged Changes
            </span>
            <div className="flex items-center gap-1.5">
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-5 w-5"
                    onClick={handleDiscardAll}
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Discard all unstaged</TooltipContent>
              </Tooltip>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-5 w-5"
                    onClick={handleStageAll}
                  >
                    <Plus className="h-3.5 w-3.5" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Stage all</TooltipContent>
              </Tooltip>
              <span className="text-xs text-muted-foreground">
                {unstaged.length}
              </span>
            </div>
          </div>

          {/* Unstaged file rows. */}
          {unstaged.map((file) => {
            const isConflicted = file.status === "conflicted";
            return (
              <div
                key={file.path}
                className="group flex items-center px-2 py-1 rounded-md hover:bg-muted/50 cursor-pointer gap-1.5"
                onClick={() => {
                  if (isConflicted) {
                    void handleConflictClick(file);
                  } else {
                    onFileClick(file);
                  }
                }}
              >
                {isConflicted && (
                  <AlertTriangle className="h-3.5 w-3.5 text-destructive shrink-0" />
                )}
                <span className="text-sm truncate flex-1 min-w-0">
                  {file.path}
                </span>
                <span
                  className={`text-xs font-medium shrink-0 ${GIT_STATUS_CONFIG[file.status].className}`}
                >
                  {GIT_STATUS_CONFIG[file.status].label}
                </span>
                {!isConflicted && (
                  <div className="hidden gap-0.5 shrink-0 group-hover:flex">
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-5 w-5"
                          onClick={(e) => {
                            e.stopPropagation();
                            discardFiles([file.path]);
                          }}
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent>Discard</TooltipContent>
                    </Tooltip>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-5 w-5"
                          onClick={(e) => {
                            e.stopPropagation();
                            stageFiles([file.path]);
                          }}
                        >
                          <Plus className="h-3.5 w-3.5" />
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent>Stage</TooltipContent>
                    </Tooltip>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </ScrollArea>
    </TooltipProvider>
  );
}
```

- [ ] **Step 2: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/git/GitFileList.tsx
git commit -m "feat: route conflicted files to conflict editor in GitFileList"
```

---

## Task 3: GitPanel — Conflict Right-Panel View and In-Merge Banner

**Files:**
- Modify: `src/components/git/GitPanel.tsx`

- [ ] **Step 1: Apply the change**

Replace the entire file contents with:

```tsx
import { useState, useEffect, useCallback } from "react";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { GitCommitForm } from "@/components/git/GitCommitForm";
import { GitCommitLog } from "@/components/git/GitCommitLog";
import { GitStashSection } from "@/components/git/GitStashSection";
import { GitCredentialsDialog } from "@/components/git/GitCredentialsDialog";
import { GitRemotesDialog } from "@/components/git/GitRemotesDialog";
import { GitCloneDialog } from "@/components/git/GitCloneDialog";
import { GitLandingPanel } from "@/components/git/GitLandingPanel";
import { GitLinksSection } from "@/components/git/GitLinksSection";
import { GitFileList } from "@/components/git/GitFileList";
import { DiffViewForFile } from "@/components/git/DiffViewForFile";
import { ConflictResolver } from "@/components/git/ConflictResolver";
import { BranchSelector } from "@/components/git/BranchSelector";
import {
  Collapsible,
  CollapsibleTrigger,
  CollapsibleContent,
} from "@/components/ui/collapsible";
import { Separator } from "@/components/ui/separator";
import { useGitStore } from "@/stores/git-store";
import { gitInit, gitIsRepo } from "@/lib/tauri-api";
import { Package, ChevronDown, ArrowLeft, AlertTriangle } from "lucide-react";
import type { ConflictFile, FileStatus } from "@/lib/tauri-api";

type RightPanelView =
  | { kind: "landing" }
  | { kind: "diff"; file: FileStatus }
  | { kind: "conflict"; conflictFile: ConflictFile }
  | { kind: "commits" }
  | { kind: "stashes" };

interface GitPanelProps {
  collectionPath: string;
  collectionName: string;
}

export function GitPanel({ collectionPath, collectionName }: GitPanelProps) {
  // null = loading, false = not a repo, true = is a repo.
  const [isRepo, setIsRepo] = useState<boolean | null>(null);
  const [leftWidth, setLeftWidth] = useState(320);
  const [rightPanel, setRightPanel] = useState<RightPanelView>({
    kind: "landing",
  });
  const [showRemotesDialog, setShowRemotesDialog] = useState(false);
  const [showCloneDialog, setShowCloneDialog] = useState(false);
  const [changesOpen, setChangesOpen] = useState(true);

  const { showCredentialsDialog, setCollection, refreshLog, status } = useGitStore();

  const hasConflicts = (status?.files.some((f) => f.status === "conflicted")) ?? false;
  const conflictCount = status?.files.filter((f) => f.status === "conflicted").length ?? 0;

  // Check git repo status and initialize the git store when the path is known.
  const checkAndLoad = useCallback(
    async (path: string) => {
      setIsRepo(null);
      try {
        const repo = await gitIsRepo(path);
        setIsRepo(repo);
        if (repo) {
          await setCollection(path);
        }
      } catch {
        setIsRepo(false);
      }
    },
    [setCollection],
  );

  useEffect(() => {
    void checkAndLoad(collectionPath);
  }, [collectionPath, checkAndLoad]);

  // Load the commit log when the commits view is opened.
  useEffect(() => {
    if (rightPanel.kind === "commits") void refreshLog();
  }, [rightPanel.kind, refreshLog]);

  if (isRepo === null) {
    return (
      <div className="flex items-center justify-center h-full text-sm text-muted-foreground">
        Loading...
      </div>
    );
  }

  if (!isRepo) {
    return (
      <div className="flex flex-col items-center justify-center gap-3 h-full px-4 text-center">
        <p className="text-sm text-muted-foreground">
          This collection is not a Git repository.
        </p>
        <div className="flex gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={async () => {
              await gitInit(collectionPath);
              await checkAndLoad(collectionPath);
            }}
          >
            Initialize Git
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => setShowCloneDialog(true)}
          >
            Clone Repository
          </Button>
        </div>
        {showCredentialsDialog && <GitCredentialsDialog />}
        <GitCloneDialog
          open={showCloneDialog}
          onOpenChange={setShowCloneDialog}
        />
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 flex overflow-hidden">
        {/* LEFT PANEL */}
        <div
          style={{ width: `${leftWidth}px` }}
          className="shrink-0 border-r border-border/70 flex flex-col overflow-hidden"
        >
          {/* Collection name header with branch selector. */}
          <div className="flex items-center gap-2 px-3 py-2.5 border-b border-border/70 shrink-0">
            <Package className="h-3.5 w-3.5 text-muted-foreground" />
            <span className="text-sm font-medium truncate flex-1">
              {collectionName}
            </span>
            <BranchSelector />
          </div>

          {/* In-merge banner — shown when there are conflicted files. */}
          {hasConflicts && (
            <div className="px-3 py-2 bg-destructive/10 border-b border-border/70 flex items-center gap-2 shrink-0">
              <AlertTriangle className="h-3.5 w-3.5 text-destructive shrink-0" />
              <span className="text-xs text-destructive flex-1">
                Merge in progress — {conflictCount} conflicted
              </span>
            </div>
          )}

          {/* Changes section with commit form */}
          <div className="shrink-0 px-3 pt-2.5 pb-2 space-y-2 border-b border-border/70">
            <Collapsible open={changesOpen} onOpenChange={setChangesOpen}>
              <CollapsibleTrigger className="flex items-center gap-1 text-sm font-medium text-primary">
                <ChevronDown
                  className={`h-3.5 w-3.5 transition-transform ${!changesOpen ? "-rotate-90" : ""}`}
                />
                Changes
              </CollapsibleTrigger>
              <CollapsibleContent className="pt-2 space-y-2">
                <GitCommitForm />
              </CollapsibleContent>
            </Collapsible>
          </div>

          {/* File list */}
          <GitFileList
            onFileClick={(file) => setRightPanel({ kind: "diff", file })}
            onConflictClick={(conflictFile) =>
              setRightPanel({ kind: "conflict", conflictFile })
            }
          />

          {/* Links section */}
          <div className="shrink-0 border-t border-border/70">
            <GitLinksSection
              onNavigate={(view) => setRightPanel({ kind: view })}
              onOpenRemotes={() => setShowRemotesDialog(true)}
            />
          </div>
        </div>

        {/* Resize handle. */}
        <div
          role="separator"
          className="w-1.5 shrink-0 cursor-col-resize bg-border/35 transition-colors hover:bg-primary/35"
          onPointerDown={(e) => {
            e.preventDefault();
            const startX = e.clientX;
            const startWidth = leftWidth;
            const onMove = (ev: PointerEvent) => {
              setLeftWidth(Math.min(500, Math.max(200, startWidth + ev.clientX - startX)));
            };
            const onUp = () => {
              window.removeEventListener("pointermove", onMove);
              window.removeEventListener("pointerup", onUp);
            };
            window.addEventListener("pointermove", onMove);
            window.addEventListener("pointerup", onUp);
          }}
        />

        {/* RIGHT PANEL */}
        <div className="flex-1 overflow-hidden flex flex-col">
          {/* Breadcrumb header — visible when not on landing/overview. */}
          {rightPanel.kind !== "landing" && (
            <div className="flex items-center gap-2 px-3 py-2 border-b border-border/70 shrink-0">
              <Button
                variant="ghost"
                size="sm"
                className="h-7 gap-1.5 text-xs"
                onClick={() => setRightPanel({ kind: "landing" })}
              >
                <ArrowLeft className="h-3.5 w-3.5" />
                Overview
              </Button>
              <Separator orientation="vertical" className="h-4" />
              <span className="text-xs text-muted-foreground truncate">
                {rightPanel.kind === "diff" && rightPanel.file.path}
                {rightPanel.kind === "conflict" && rightPanel.conflictFile.path}
                {rightPanel.kind === "commits" && "Commit History"}
                {rightPanel.kind === "stashes" && "Stashes"}
              </span>
            </div>
          )}

          {/* Right panel content. */}
          <div className="flex-1 overflow-hidden">
            {rightPanel.kind === "landing" && <GitLandingPanel />}
            {rightPanel.kind === "diff" && (
              <DiffViewForFile
                file={rightPanel.file}
                collectionPath={collectionPath}
              />
            )}
            {rightPanel.kind === "conflict" && (
              <ConflictResolver
                conflictState={{
                  filePath: rightPanel.conflictFile.path,
                  collectionPath: collectionPath,
                  ours: rightPanel.conflictFile.ours,
                  theirs: rightPanel.conflictFile.theirs,
                  ancestor: rightPanel.conflictFile.ancestor ?? null,
                }}
              />
            )}
            {rightPanel.kind === "commits" && <GitCommitLog />}
            {rightPanel.kind === "stashes" && (
              <ScrollArea className="h-full">
                <div className="p-4">
                  <GitStashSection />
                </div>
              </ScrollArea>
            )}
          </div>
        </div>
      </div>

      {/* Dialogs */}
      {showCredentialsDialog && <GitCredentialsDialog />}
      <GitRemotesDialog
        open={showRemotesDialog}
        onOpenChange={setShowRemotesDialog}
      />
      <GitCloneDialog
        open={showCloneDialog}
        onOpenChange={setShowCloneDialog}
      />
    </div>
  );
}
```

- [ ] **Step 2: Check ConflictResolver prop shape**

```bash
grep -n "conflictState\|ConflictState\|ancestor" src/components/git/ConflictResolver.tsx | head -20
```

Expected: `conflictState` prop accepts `{ filePath, collectionPath, ours, theirs, ancestor: string | null }`. If the field is named differently, update the prop accordingly.

- [ ] **Step 3: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/git/GitPanel.tsx
git commit -m "feat: add conflict right-panel view and in-merge banner to GitPanel"
```
