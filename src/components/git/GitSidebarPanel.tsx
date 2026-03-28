import { useState, useEffect } from 'react';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { GitCommitForm } from './GitCommitForm';
import { GitStagedFiles } from './GitStagedFiles';
import { GitChangedFiles } from './GitChangedFiles';
import { GitStashSection } from './GitStashSection';
import { GitCommitLog } from './GitCommitLog';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Button } from '@/components/ui/button';
import { Separator } from '@/components/ui/separator';
import { useGitStore } from '@/stores/git-store';
import { gitInit } from '@/lib/tauri-api';

// Git panel shown in the sidebar Git tab.
export function GitSidebarPanel() {
  const { isRepo, collectionPath, loading, setCollection, refreshLog } = useGitStore();
  const [activeSubTab, setActiveSubTab] = useState('changes');

  useEffect(() => {
    if (activeSubTab === 'log') refreshLog();
  }, [activeSubTab, refreshLog]);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-32 text-sm text-muted-foreground">
        Loading...
      </div>
    );
  }

  if (!isRepo) {
    return (
      <div className="flex flex-col items-center justify-center gap-3 h-32 px-4 text-center">
        <p className="text-sm text-muted-foreground">Not a git repository</p>
        {collectionPath && (
          <Button
            variant="outline"
            size="sm"
            onClick={async () => {
              await gitInit(collectionPath);
              await setCollection(collectionPath);
            }}
          >
            Initialize Git
          </Button>
        )}
      </div>
    );
  }

  return (
    <Tabs value={activeSubTab} onValueChange={setActiveSubTab} className="h-full flex flex-col">
      <TabsList className="w-full shrink-0">
        <TabsTrigger value="changes" className="flex-1 text-xs">Changes</TabsTrigger>
        <TabsTrigger value="log" className="flex-1 text-xs">Log</TabsTrigger>
        <TabsTrigger value="stash" className="flex-1 text-xs">Stash</TabsTrigger>
      </TabsList>
      <TabsContent value="changes" className="flex-1 overflow-hidden">
        <ScrollArea className="h-full">
          <div className="space-y-2 p-2">
            <GitCommitForm />
            <Separator />
            <GitStagedFiles />
            <GitChangedFiles />
          </div>
        </ScrollArea>
      </TabsContent>
      <TabsContent value="log" className="flex-1 overflow-hidden">
        <GitCommitLog />
      </TabsContent>
      <TabsContent value="stash" className="flex-1 overflow-hidden">
        <ScrollArea className="h-full">
          <div className="p-2">
            <GitStashSection />
          </div>
        </ScrollArea>
      </TabsContent>
    </Tabs>
  );
}
