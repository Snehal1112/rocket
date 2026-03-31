import { useState, useEffect, useCallback } from 'react';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { GitCommitForm } from '@/components/git/GitCommitForm';
import { GitCommitLog } from '@/components/git/GitCommitLog';
import { GitStashSection } from '@/components/git/GitStashSection';
import { GitCredentialsDialog } from '@/components/git/GitCredentialsDialog';
import { GitRemotesDialog } from '@/components/git/GitRemotesDialog';
import { GitCloneDialog } from '@/components/git/GitCloneDialog';
import { GitLandingPanel } from '@/components/git/GitLandingPanel';
import { GitLinksSection } from '@/components/git/GitLinksSection';
import { GitFileList } from '@/components/git/GitFileList';
import { DiffViewForFile } from '@/components/git/DiffViewForFile';
import { BranchSelector } from '@/components/git/BranchSelector';
import { Collapsible, CollapsibleTrigger, CollapsibleContent } from '@/components/ui/collapsible';
import { useGitStore } from '@/stores/git-store';
import { gitInit, gitIsRepo } from '@/lib/tauri-api';
import { Package, ChevronDown } from 'lucide-react';
import type { FileStatus } from '@/lib/tauri-api';

type RightPanelView =
  | { kind: 'landing' }
  | { kind: 'diff'; file: FileStatus }
  | { kind: 'commits' }
  | { kind: 'stashes' };

interface GitPanelProps {
  collectionPath: string;
  collectionName: string;
}

export function GitPanel({ collectionPath, collectionName }: GitPanelProps) {
  // null = loading, false = not a repo, true = is a repo.
  const [isRepo, setIsRepo] = useState<boolean | null>(null);
  const [rightPanel, setRightPanel] = useState<RightPanelView>({ kind: 'landing' });
  const [showRemotesDialog, setShowRemotesDialog] = useState(false);
  const [showCloneDialog, setShowCloneDialog] = useState(false);
  const [changesOpen, setChangesOpen] = useState(true);

  const { showCredentialsDialog, setCollection, refreshLog } = useGitStore();

  // Check git repo status and initialize the git store when the path is known.
  const checkAndLoad = useCallback(async (path: string) => {
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
  }, [setCollection]);

  useEffect(() => {
    void checkAndLoad(collectionPath);
  }, [collectionPath, checkAndLoad]);

  // Load the commit log when the commits view is opened.
  useEffect(() => {
    if (rightPanel.kind === 'commits') void refreshLog();
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
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 flex overflow-hidden">

        {/* LEFT PANEL */}
        <div className="w-80 border-r border-border/70 flex flex-col overflow-hidden">

          {/* Collection name header with branch selector. */}
          <div className="flex items-center gap-2 px-3 py-2.5 border-b border-border/70 shrink-0">
            <Package className="h-3.5 w-3.5 text-muted-foreground" />
            <span className="text-sm font-medium truncate flex-1">
              {collectionName}
            </span>
            <BranchSelector />
          </div>

          {/* Changes section with commit form */}
          <div className="shrink-0 px-3 pt-3 pb-2 space-y-2 border-b border-border/70">
            <Collapsible open={changesOpen} onOpenChange={setChangesOpen}>
              <CollapsibleTrigger className="flex items-center gap-1 text-sm font-medium text-primary">
                <ChevronDown className={`h-3 w-3 transition-transform ${!changesOpen ? '-rotate-90' : ''}`} />
                Changes
              </CollapsibleTrigger>
              <CollapsibleContent className="pt-2 space-y-2">
                <GitCommitForm />
              </CollapsibleContent>
            </Collapsible>
          </div>

          {/* File list */}
          <GitFileList
            onFileClick={(file) => setRightPanel({ kind: 'diff', file })}
          />

          {/* Links section */}
          <div className="shrink-0 border-t border-border/70">
            <GitLinksSection
              onNavigate={(view) => setRightPanel({ kind: view })}
              onOpenRemotes={() => setShowRemotesDialog(true)}
            />
          </div>
        </div>

        {/* RIGHT PANEL */}
        <div className="flex-1 overflow-hidden">
          {rightPanel.kind === 'landing' && <GitLandingPanel />}
          {rightPanel.kind === 'diff' && (
            <DiffViewForFile file={rightPanel.file} collectionPath={collectionPath} />
          )}
          {rightPanel.kind === 'commits' && <GitCommitLog />}
          {rightPanel.kind === 'stashes' && (
            <ScrollArea className="h-full">
              <div className="p-4">
                <GitStashSection />
              </div>
            </ScrollArea>
          )}
        </div>

      </div>

      {/* Dialogs */}
      {showCredentialsDialog && <GitCredentialsDialog />}
      <GitRemotesDialog open={showRemotesDialog} onOpenChange={setShowRemotesDialog} />
      <GitCloneDialog open={showCloneDialog} onOpenChange={setShowCloneDialog} />
    </div>
  );
}
