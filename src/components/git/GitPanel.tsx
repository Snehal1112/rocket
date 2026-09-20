import { AlertTriangle, ArrowLeft, Package } from 'lucide-react';
import { lazy, Suspense, useCallback, useEffect, useRef, useState } from 'react';
import { useStore } from 'zustand';
import { BranchSelector } from '@/components/git/BranchSelector';

// Lazy-load ConflictResolver — it pulls in Monaco and only renders on merge conflicts.
const ConflictResolver = lazy(() =>
  import('@/components/git/ConflictResolver').then((m) => ({ default: m.ConflictResolver })),
);

import { CommitDiffView } from '@/components/git/CommitDiffView';
import { DiffViewForFile } from '@/components/git/DiffViewForFile';
import { GitCloneDialog } from '@/components/git/GitCloneDialog';
import { GitCommitForm } from '@/components/git/GitCommitForm';
import { GitCommitLog } from '@/components/git/GitCommitLog';
import { GitCredentialsDialog } from '@/components/git/GitCredentialsDialog';
import { GitFileList } from '@/components/git/GitFileList';
import { GitIdentityDialog } from '@/components/git/GitIdentityDialog';
import { GitLandingPanel } from '@/components/git/GitLandingPanel';
import { GitLinksSection } from '@/components/git/GitLinksSection';
import { GitPanelSkeleton } from '@/components/git/GitPanelSkeleton';
import { GitRemotesDialog } from '@/components/git/GitRemotesDialog';
import { GitStashSection } from '@/components/git/GitStashSection';
import { Button } from '@/components/ui/button';
import { Separator } from '@/components/ui/separator';
import type { CommitInfo, ConflictFile, FileDiff, FileStatus } from '@/lib/tauri-api';
import { gitDiffCommit, gitSetIdentity, onCollectionChanged } from '@/lib/tauri-api';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';

type RightPanelView =
  | { kind: 'landing' }
  | { kind: 'diff'; file: FileStatus }
  | { kind: 'conflict'; conflictFile: ConflictFile }
  | { kind: 'commits' }
  | { kind: 'commitDiff'; commit: CommitInfo; diffs: FileDiff[] }
  | { kind: 'stashes' };

interface GitPanelProps {
  repositoryId: string;
  repositoryLabel: string;
}

export function GitPanel({ repositoryId, repositoryLabel }: GitPanelProps) {
  const [leftWidth, setLeftWidth] = useState(320);
  const [rightPanel, setRightPanel] = useState<RightPanelView>({
    kind: 'landing',
  });
  const [showRemotesDialog, setShowRemotesDialog] = useState(false);
  const [showCloneDialog, setShowCloneDialog] = useState(false);
  const [store] = useState(() => createGitStore());

  const showCredentialsDialog = useStore(store, (state) => state.showCredentialsDialog);
  const setRepository = useStore(store, (state) => state.setRepository);
  const refreshLog = useStore(store, (state) => state.refreshLog);
  const refreshStashes = useStore(store, (state) => state.refreshStashes);
  const refreshStatus = useStore(store, (state) => state.refreshStatus);
  const status = useStore(store, (state) => state.status);
  const loadedRepositoryId = useStore(store, (state) => state.repositoryId);
  const loadStatus = useStore(store, (state) => state.loadStatus);
  const loadError = useStore(store, (state) => state.error);
  const initRepo = useStore(store, (state) => state.initRepo);
  const showIdentitySetupDialog = useStore(store, (state) => state.showIdentitySetupDialog);
  const identitySetupInitialName = useStore(store, (state) => state.identitySetupInitialName);
  const identitySetupInitialEmail = useStore(store, (state) => state.identitySetupInitialEmail);
  const activatePendingCredentials = useStore(store, (state) => state.activatePendingCredentials);
  const currentBranch = status?.branch ?? null;
  const hasConflicts = status?.files.some((f) => f.status === 'conflicted') ?? false;
  const conflictCount = status?.files.filter((f) => f.status === 'conflicted').length ?? 0;

  useEffect(() => {
    if (loadedRepositoryId === repositoryId) return;
    void setRepository(repositoryId);
  }, [repositoryId, loadedRepositoryId, setRepository]);

  // Keyboard handler for the vertical separator: ArrowLeft/ArrowRight adjust width.
  const handleSeparatorKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'ArrowLeft') {
      e.preventDefault();
      setLeftWidth((w) => Math.min(500, Math.max(200, w - 20)));
    } else if (e.key === 'ArrowRight') {
      e.preventDefault();
      setLeftWidth((w) => Math.min(500, Math.max(200, w + 20)));
    }
  }, []);

  const handleIdentitySetupConfirm = async (name: string, email: string) => {
    try {
      await gitSetIdentity(repositoryId, name, email);
    } catch {
      // Non-blocking — proceed even if identity save fails.
    }
    activatePendingCredentials();
  };

  const handleIdentitySetupCancel = () => {
    activatePendingCredentials();
  };

  const handleCommitClick = async (commit: CommitInfo) => {
    try {
      const diffs = await gitDiffCommit(repositoryId, commit.fullId);
      setRightPanel({ kind: 'commitDiff', commit, diffs });
    } catch {
      // If diff fails, silently stay on commits view.
    }
  };

  // Load the commit log when the commits view is opened.
  useEffect(() => {
    if (rightPanel.kind === 'commits') void refreshLog();
  }, [rightPanel.kind, refreshLog]);

  // Refresh the stash list when the stash view is opened.
  useEffect(() => {
    if (rightPanel.kind === 'stashes') void refreshStashes();
  }, [rightPanel.kind, refreshStashes]);

  // Refresh git status when collection files change (e.g. delete/rename in sidebar).
  // Debounced so rapid file-watcher events during a branch checkout collapse into
  // one call. Git operations are skipped — the store refreshes inline after each one.
  const statusDebounce = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    if (loadStatus !== 'ready') return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onCollectionChanged((event) => {
      if (event.type === 'branchSwitched' || event.type === 'branchMerged') return;
      if (statusDebounce.current) clearTimeout(statusDebounce.current);
      statusDebounce.current = setTimeout(() => void refreshStatus(), 300);
    }).then((fn) => {
      if (cancelled) {
        // Cleanup already ran before registration resolved — the effect's own
        // `unlisten` variable will never be read again, so unregister directly.
        fn();
        return;
      }
      unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
      if (statusDebounce.current) clearTimeout(statusDebounce.current);
    };
  }, [loadStatus, refreshStatus]);

  // Return to the overview when the branch changes so stale diff/conflict views
  // from the previous branch are not shown.
  const prevBranchRef = useRef<string | null>(null);
  useEffect(() => {
    if (
      currentBranch !== null &&
      prevBranchRef.current !== null &&
      prevBranchRef.current !== currentBranch
    ) {
      setRightPanel({ kind: 'landing' });
    }
    prevBranchRef.current = currentBranch;
  }, [currentBranch]);

  if (loadStatus === 'idle' || loadStatus === 'loading') {
    return <GitPanelSkeleton />;
  }

  if (loadStatus === 'error') {
    return (
      <GitStoreProvider store={store}>
        <div className='flex flex-col items-center justify-center gap-3 h-full px-4 text-center'>
          <AlertTriangle className='h-5 w-5 text-destructive' />
          <p className='text-sm text-destructive'>Failed to load this repository.</p>
          {loadError && (
            <p className='text-xs text-muted-foreground wrap-break-word max-w-sm'>{loadError}</p>
          )}
          <Button variant='outline' size='sm' onClick={() => void setRepository(repositoryId)}>
            Retry
          </Button>
        </div>
      </GitStoreProvider>
    );
  }

  if (loadStatus === 'not-repo') {
    return (
      <GitStoreProvider store={store}>
        <div className='flex flex-col items-center justify-center gap-3 h-full px-4 text-center'>
          <p className='text-sm text-muted-foreground'>This collection is not a Git repository.</p>
          {loadError && (
            <p className='text-xs text-destructive wrap-break-word max-w-sm'>{loadError}</p>
          )}
          <div className='flex gap-2'>
            <Button variant='outline' size='sm' onClick={() => void initRepo(repositoryId)}>
              Initialize Git
            </Button>
            <Button variant='outline' size='sm' onClick={() => setShowCloneDialog(true)}>
              Clone Repository
            </Button>
          </div>
          {showCredentialsDialog && <GitCredentialsDialog />}
          {showIdentitySetupDialog && (
            <GitIdentityDialog
              open={showIdentitySetupDialog}
              onConfirm={handleIdentitySetupConfirm}
              onCancel={handleIdentitySetupCancel}
              initialName={identitySetupInitialName}
              initialEmail={identitySetupInitialEmail}
              confirmLabel='Save Identity'
            />
          )}
          <GitCloneDialog open={showCloneDialog} onOpenChange={setShowCloneDialog} />
        </div>
      </GitStoreProvider>
    );
  }

  // loadStatus === 'ready' from here.
  return (
    <GitStoreProvider store={store}>
      <div className='flex flex-col h-full'>
        <div className='flex-1 flex overflow-hidden'>
          {/* LEFT PANEL */}
          <div
            style={{ width: `${leftWidth}px` }}
            className='shrink-0 border-r border-border/70 flex flex-col overflow-hidden'
          >
            {/* Collection name header with branch selector. */}
            <div className='flex items-center gap-2 px-3 py-2.5 border-b border-border/70 shrink-0'>
              <Package className='h-3.5 w-3.5 text-muted-foreground' />
              <span className='text-sm font-medium truncate flex-1'>{repositoryLabel}</span>
              <BranchSelector />
            </div>

            {/* In-merge banner — shown when there are conflicted files. */}
            {hasConflicts && (
              <div className='px-3 py-2 bg-destructive/10 border-b border-border/70 flex items-center gap-2 shrink-0'>
                <AlertTriangle className='h-3.5 w-3.5 text-destructive shrink-0' />
                <span className='text-xs text-destructive flex-1'>
                  Merge in progress — {conflictCount} conflicted
                </span>
              </div>
            )}

            {/* Commit form */}
            <div className='shrink-0 px-3 pt-2.5 pb-2 border-b border-border/70'>
              <GitCommitForm />
            </div>

            {/* File list */}
            <GitFileList
              onFileClick={(file) => setRightPanel({ kind: 'diff', file })}
              onConflictClick={(conflictFile) => setRightPanel({ kind: 'conflict', conflictFile })}
            />

            {/* Links section */}
            <div className='shrink-0 border-t border-border/70'>
              <GitLinksSection
                onNavigate={(view) => setRightPanel({ kind: view })}
                onOpenRemotes={() => setShowRemotesDialog(true)}
              />
            </div>
          </div>

          {/* Resize handle. */}
          {/* biome-ignore lint/a11y/useSemanticElements: drag splitter cannot be an <hr> */}
          <div
            role='separator'
            tabIndex={0}
            aria-orientation='vertical'
            aria-valuemin={200}
            aria-valuemax={500}
            aria-valuenow={leftWidth}
            className='w-1.5 shrink-0 cursor-col-resize bg-border/35 transition-colors hover:bg-primary/35'
            onPointerDown={(e) => {
              e.preventDefault();
              const startX = e.clientX;
              const startWidth = leftWidth;
              const onMove = (ev: PointerEvent) => {
                setLeftWidth(Math.min(500, Math.max(200, startWidth + ev.clientX - startX)));
              };
              const onUp = () => {
                window.removeEventListener('pointermove', onMove);
                window.removeEventListener('pointerup', onUp);
              };
              window.addEventListener('pointermove', onMove);
              window.addEventListener('pointerup', onUp);
            }}
            onKeyDown={handleSeparatorKeyDown}
          />

          {/* RIGHT PANEL */}
          <div className='flex-1 overflow-hidden flex flex-col'>
            {/* Breadcrumb header — visible when not on landing/overview. */}
            {rightPanel.kind !== 'landing' && (
              <div className='flex items-center gap-2 px-3 py-2 border-b border-border/70 shrink-0'>
                <Button
                  variant='ghost'
                  size='sm'
                  className='h-7 gap-1.5 text-xs'
                  onClick={() => setRightPanel({ kind: 'landing' })}
                >
                  <ArrowLeft className='h-3.5 w-3.5' />
                  Overview
                </Button>
                <Separator orientation='vertical' className='h-4' />
                <span className='text-xs text-muted-foreground truncate'>
                  {rightPanel.kind === 'diff' && rightPanel.file.path}
                  {rightPanel.kind === 'conflict' && rightPanel.conflictFile.path}
                  {rightPanel.kind === 'commits' && 'Commit History'}
                  {rightPanel.kind === 'commitDiff' &&
                    `${rightPanel.commit.id} — ${rightPanel.commit.message.slice(0, 40)}`}
                  {rightPanel.kind === 'stashes' && 'Stashes'}
                </span>
              </div>
            )}

            {/* Right panel content. */}
            <div className='flex-1 overflow-hidden'>
              {rightPanel.kind === 'landing' && <GitLandingPanel />}
              {rightPanel.kind === 'diff' && (
                <DiffViewForFile
                  file={rightPanel.file}
                  repositoryId={repositoryId}
                  repositoryLabel={repositoryLabel}
                />
              )}
              {rightPanel.kind === 'conflict' && (
                <Suspense fallback={null}>
                  <ConflictResolver
                    conflictState={{
                      filePath: rightPanel.conflictFile.path,
                      repositoryId,
                      repositoryLabel,
                      ours: rightPanel.conflictFile.ours,
                      theirs: rightPanel.conflictFile.theirs,
                      ancestor: rightPanel.conflictFile.ancestor ?? null,
                    }}
                    onResolved={() => setRightPanel({ kind: 'landing' })}
                  />
                </Suspense>
              )}
              {rightPanel.kind === 'commits' && <GitCommitLog onCommitClick={handleCommitClick} />}
              {rightPanel.kind === 'commitDiff' && (
                <CommitDiffView
                  diffs={rightPanel.diffs}
                  repositoryId={repositoryId}
                  repositoryLabel={repositoryLabel}
                />
              )}
              {rightPanel.kind === 'stashes' && (
                <div className='overflow-y-auto h-full'>
                  <div className='p-4'>
                    <GitStashSection />
                  </div>
                </div>
              )}
            </div>
          </div>
        </div>

        {/* Dialogs */}
        {showCredentialsDialog && <GitCredentialsDialog />}
        {showIdentitySetupDialog && (
          <GitIdentityDialog
            open={showIdentitySetupDialog}
            onConfirm={handleIdentitySetupConfirm}
            onCancel={handleIdentitySetupCancel}
            initialName={identitySetupInitialName}
            initialEmail={identitySetupInitialEmail}
            confirmLabel='Save Identity'
          />
        )}
        <GitRemotesDialog open={showRemotesDialog} onOpenChange={setShowRemotesDialog} />
        <GitCloneDialog open={showCloneDialog} onOpenChange={setShowCloneDialog} />
      </div>
    </GitStoreProvider>
  );
}
