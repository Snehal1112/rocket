import { AlertTriangle, ArrowLeft, Package } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { BranchSelector } from '@/components/git/BranchSelector';
import { ConflictResolver } from '@/components/git/ConflictResolver';
import { DiffViewForFile } from '@/components/git/DiffViewForFile';
import { GitCloneDialog } from '@/components/git/GitCloneDialog';
import { GitCommitForm } from '@/components/git/GitCommitForm';
import { GitCommitLog } from '@/components/git/GitCommitLog';
import { GitCredentialsDialog } from '@/components/git/GitCredentialsDialog';
import { GitFileList } from '@/components/git/GitFileList';
import { GitLandingPanel } from '@/components/git/GitLandingPanel';
import { GitLinksSection } from '@/components/git/GitLinksSection';
import { GitRemotesDialog } from '@/components/git/GitRemotesDialog';
import { GitStashSection } from '@/components/git/GitStashSection';
import { Button } from '@/components/ui/button';
import { Separator } from '@/components/ui/separator';
import type { ConflictFile, FileStatus } from '@/lib/tauri-api';
import { gitInit, gitIsRepo } from '@/lib/tauri-api';
import { useGitStore } from '@/stores/git-store';

type RightPanelView =
  | { kind: 'landing' }
  | { kind: 'diff'; file: FileStatus }
  | { kind: 'conflict'; conflictFile: ConflictFile }
  | { kind: 'commits' }
  | { kind: 'stashes' };

interface GitPanelProps {
  collectionPath: string;
  collectionName: string;
}

export function GitPanel({ collectionPath, collectionName }: GitPanelProps) {
  // null = loading, false = not a repo, true = is a repo.
  const [isRepo, setIsRepo] = useState<boolean | null>(null);
  const [leftWidth, setLeftWidth] = useState(320);
  const [rightPanel, setRightPanel] = useState<RightPanelView>({
    kind: 'landing',
  });
  const [showRemotesDialog, setShowRemotesDialog] = useState(false);
  const [showCloneDialog, setShowCloneDialog] = useState(false);

  const { showCredentialsDialog, setCollection, refreshLog, refreshStashes, status } =
    useGitStore();
  const hasConflicts = status?.files.some((f) => f.status === 'conflicted') ?? false;
  const conflictCount = status?.files.filter((f) => f.status === 'conflicted').length ?? 0;

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
    if (rightPanel.kind === 'commits') void refreshLog();
  }, [rightPanel.kind, refreshLog]);

  // Refresh the stash list when the stash view is opened.
  useEffect(() => {
    if (rightPanel.kind === 'stashes') void refreshStashes();
  }, [rightPanel.kind, refreshStashes]);

  if (isRepo === null) {
    return (
      <div className='flex items-center justify-center h-full text-sm text-muted-foreground'>
        Loading...
      </div>
    );
  }

  if (!isRepo) {
    return (
      <div className='flex flex-col items-center justify-center gap-3 h-full px-4 text-center'>
        <p className='text-sm text-muted-foreground'>This collection is not a Git repository.</p>
        <div className='flex gap-2'>
          <Button
            variant='outline'
            size='sm'
            onClick={async () => {
              await gitInit(collectionPath);
              await checkAndLoad(collectionPath);
            }}
          >
            Initialize Git
          </Button>
          <Button variant='outline' size='sm' onClick={() => setShowCloneDialog(true)}>
            Clone Repository
          </Button>
        </div>
        {showCredentialsDialog && <GitCredentialsDialog />}
        <GitCloneDialog open={showCloneDialog} onOpenChange={setShowCloneDialog} />
      </div>
    );
  }

  return (
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
            <span className='text-sm font-medium truncate flex-1'>{collectionName}</span>
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
                {rightPanel.kind === 'stashes' && 'Stashes'}
              </span>
            </div>
          )}

          {/* Right panel content. */}
          <div className='flex-1 overflow-hidden'>
            {rightPanel.kind === 'landing' && <GitLandingPanel />}
            {rightPanel.kind === 'diff' && (
              <DiffViewForFile file={rightPanel.file} collectionPath={collectionPath} />
            )}
            {rightPanel.kind === 'conflict' && (
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
            {rightPanel.kind === 'commits' && <GitCommitLog />}
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
      <GitRemotesDialog open={showRemotesDialog} onOpenChange={setShowRemotesDialog} />
      <GitCloneDialog open={showCloneDialog} onOpenChange={setShowCloneDialog} />
    </div>
  );
}
