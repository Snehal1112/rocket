import { useState, useEffect, useCallback } from 'react';
import { Button } from '@/components/ui/button';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Separator } from '@/components/ui/separator';
import { BranchSelector } from '@/components/git/BranchSelector';
import { GitRemoteActions } from '@/components/git/GitRemoteActions';
import { GitCommitForm } from '@/components/git/GitCommitForm';
import { GitStagedFiles } from '@/components/git/GitStagedFiles';
import { GitChangedFiles } from '@/components/git/GitChangedFiles';
import { GitCommitLog } from '@/components/git/GitCommitLog';
import { GitStashSection } from '@/components/git/GitStashSection';
import { GitCredentialsDialog } from '@/components/git/GitCredentialsDialog';
import { useGitStore } from '@/stores/git-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import { gitInit, gitIsRepo } from '@/lib/tauri-api';

interface WorkspaceGitTabProps {
  workspaceId: string;
}

export function WorkspaceGitTab({ workspaceId }: WorkspaceGitTabProps) {
  // null = loading, false = not a repo, true = is a repo
  const [isRepo, setIsRepo] = useState<boolean | null>(null);
  const [activeSubTab, setActiveSubTab] = useState<string>('changes');

  const workspaces = useWorkspaceStore((s) => s.workspaces);
  const workspace = workspaces.find((w) => w.id === workspaceId);
  const workspacePath = workspace?.path ?? null;

  const { status, showCredentialsDialog, setCollection, refreshLog } = useGitStore();

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
    if (workspacePath) {
      void checkAndLoad(workspacePath);
    }
  }, [workspacePath, checkAndLoad]);

  // Refresh the commit log when switching to the log sub-tab.
  useEffect(() => {
    if (activeSubTab === 'log') void refreshLog();
  }, [activeSubTab, refreshLog]);

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
          This workspace is not a Git repository.
        </p>
        {workspacePath && (
          <Button
            variant="outline"
            size="sm"
            onClick={async () => {
              await gitInit(workspacePath);
              await checkAndLoad(workspacePath);
            }}
          >
            Initialize Git
          </Button>
        )}
      </div>
    );
  }

  const changedCount = status?.files.filter((f) => f.status !== 'unchanged').length ?? 0;

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Top bar: branch selector + remote actions. */}
      <div className="flex items-center justify-between px-4 py-2 border-b border-border/70 shrink-0">
        <div className="flex items-center gap-2">
          <BranchSelector />
          {status && !status.isClean && (
            <span className="text-xs text-muted-foreground">
              {changedCount} changed {changedCount === 1 ? 'file' : 'files'}
            </span>
          )}
        </div>
        <GitRemoteActions />
      </div>

      {/* Sub-tab layout for changes, log, and stash. */}
      <Tabs
        value={activeSubTab}
        onValueChange={setActiveSubTab}
        className="flex-1 flex flex-col overflow-hidden"
      >
        <TabsList className="w-full shrink-0 rounded-none border-b border-border/70 bg-card/60 h-9 px-3 justify-start">
          <TabsTrigger
            value="changes"
            className="text-sm rounded-none data-[state=active]:border-b-2 data-[state=active]:border-primary data-[state=active]:bg-transparent"
          >
            Changes
            {changedCount > 0 && (
              <span className="ml-1 text-2xs text-muted-foreground">({changedCount})</span>
            )}
          </TabsTrigger>
          <TabsTrigger
            value="log"
            className="text-sm rounded-none data-[state=active]:border-b-2 data-[state=active]:border-primary data-[state=active]:bg-transparent"
          >
            Log
          </TabsTrigger>
          <TabsTrigger
            value="stash"
            className="text-sm rounded-none data-[state=active]:border-b-2 data-[state=active]:border-primary data-[state=active]:bg-transparent"
          >
            Stash
          </TabsTrigger>
        </TabsList>

        <TabsContent value="changes" className="flex-1 overflow-hidden mt-0">
          <ScrollArea className="h-full">
            <div className="p-4 space-y-4 max-w-2xl">
              <GitCommitForm />
              <Separator />
              <GitStagedFiles />
              <GitChangedFiles />
            </div>
          </ScrollArea>
        </TabsContent>

        <TabsContent value="log" className="flex-1 overflow-hidden mt-0">
          <GitCommitLog />
        </TabsContent>

        <TabsContent value="stash" className="flex-1 overflow-hidden mt-0">
          <ScrollArea className="h-full">
            <div className="p-4">
              <GitStashSection />
            </div>
          </ScrollArea>
        </TabsContent>
      </Tabs>

      {showCredentialsDialog && <GitCredentialsDialog />}
    </div>
  );
}
