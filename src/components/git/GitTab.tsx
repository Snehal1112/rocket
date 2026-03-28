import { useState, useEffect } from 'react';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Separator } from '@/components/ui/separator';
import { Button } from '@/components/ui/button';
import { BranchSelector } from './BranchSelector';
import { GitRemoteActions } from './GitRemoteActions';
import { GitCommitForm } from './GitCommitForm';
import { GitStagedFiles } from './GitStagedFiles';
import { GitChangedFiles } from './GitChangedFiles';
import { GitCommitLog } from './GitCommitLog';
import { GitStashSection } from './GitStashSection';
import { GitCredentialsDialog } from './GitCredentialsDialog';
import { useGitStore } from '@/stores/git-store';
import { gitInit } from '@/lib/tauri-api';
import type { GitTab as GitTabType } from '@/types/pane-types';

interface GitTabProps {
  tab: GitTabType;
}

export function GitTab({ tab }: GitTabProps) {
  const {
    isRepo,
    loading,
    status,
    showCredentialsDialog,
    setCollection,
    refreshLog,
  } = useGitStore();

  const [activeSubTab, setActiveSubTab] = useState<string>('changes');

  // Set the git store collection when the tab mounts or collection changes.
  useEffect(() => {
    if (tab.collectionPath) {
      void setCollection(tab.collectionPath);
    }
  }, [tab.collectionPath, setCollection]);

  // Refresh log when switching to the log sub-tab.
  useEffect(() => {
    if (activeSubTab === 'log') void refreshLog();
  }, [activeSubTab, refreshLog]);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full text-sm text-muted-foreground">
        Loading git status...
      </div>
    );
  }

  if (!isRepo) {
    return (
      <div className="flex flex-col items-center justify-center gap-3 h-full px-4 text-center">
        <p className="text-sm text-muted-foreground">
          This collection is not a git repository.
        </p>
        {tab.collectionPath && (
          <Button
            variant="outline"
            size="sm"
            onClick={async () => {
              await gitInit(tab.collectionPath);
              await setCollection(tab.collectionPath);
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
      {/* Top bar: branch selector + remote actions */}
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

      {/* Body: tabbed layout */}
      <Tabs
        value={activeSubTab}
        onValueChange={setActiveSubTab}
        className="flex-1 flex flex-col overflow-hidden"
      >
        <TabsList className="w-full shrink-0 rounded-none border-b border-border/70 bg-card/60 h-9 px-3 justify-start">
          <TabsTrigger
            value="changes"
            className="text-xs rounded-none data-[state=active]:border-b-2 data-[state=active]:border-primary data-[state=active]:bg-transparent"
          >
            Changes
            {changedCount > 0 && (
              <span className="ml-1 text-2xs text-muted-foreground">({changedCount})</span>
            )}
          </TabsTrigger>
          <TabsTrigger
            value="log"
            className="text-xs rounded-none data-[state=active]:border-b-2 data-[state=active]:border-primary data-[state=active]:bg-transparent"
          >
            Log
          </TabsTrigger>
          <TabsTrigger
            value="stash"
            className="text-xs rounded-none data-[state=active]:border-b-2 data-[state=active]:border-primary data-[state=active]:bg-transparent"
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
