import { GitCommitForm } from './GitCommitForm';
import { GitStagedFiles } from './GitStagedFiles';
import { GitChangedFiles } from './GitChangedFiles';
import { GitStashSection } from './GitStashSection';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Button } from '@/components/ui/button';
import { useGitStore } from '@/stores/git-store';
import { gitInit } from '@/lib/tauri-api';
import { Separator } from '@/components/ui/separator';

// Git panel shown in the sidebar Git tab.
export function GitSidebarPanel() {
  const { isRepo, collectionPath, loading, setCollection } = useGitStore();

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
    <ScrollArea className="h-full">
      <div className="space-y-2 p-2">
        <GitCommitForm />
        <Separator />
        <GitStagedFiles />
        <GitChangedFiles />
        <Separator />
        <GitStashSection />
      </div>
    </ScrollArea>
  );
}
