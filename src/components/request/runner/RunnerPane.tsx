import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { getRunnerFolderOptions, type RunnerFolderOption } from '@/lib/runner-flatten';
import { type CollectionSummary, getCollection, listCollections } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { RunnerTab } from '@/types/pane-types';
import { RunnerRequestList } from './RunnerRequestList';
import { RunnerResultsList } from './RunnerResultsList';
import { RunnerSummaryHeader } from './RunnerSummaryHeader';

const WHOLE_COLLECTION = '__whole_collection__';

export function RunnerPane({ tab, groupId }: { tab: RunnerTab; groupId: string }) {
  const openRunnerTab = usePaneStore((s) => s.openRunnerTab);
  const closeTab = usePaneStore((s) => s.closeTab);
  const [collections, setCollections] = useState<CollectionSummary[]>([]);
  const [selectedCollection, setSelectedCollection] = useState('');
  const [selectedFolder, setSelectedFolder] = useState(WHOLE_COLLECTION);
  const [folders, setFolders] = useState<RunnerFolderOption[]>([]);
  const [isLoadingFolders, setIsLoadingFolders] = useState(false);

  useEffect(() => {
    if (tab.collectionName === null) {
      void listCollections().then(setCollections);
    }
  }, [tab.collectionName]);

  useEffect(() => {
    if (!selectedCollection) {
      setFolders([]);
      setSelectedFolder(WHOLE_COLLECTION);
      return;
    }

    let cancelled = false;
    setFolders([]);
    setSelectedFolder(WHOLE_COLLECTION);
    setIsLoadingFolders(true);
    void getCollection(selectedCollection)
      .then((collection) => {
        if (!cancelled) setFolders(getRunnerFolderOptions(collection));
      })
      .catch((error) => {
        console.error('[RunnerPane] failed to load collection folders', error);
      })
      .finally(() => {
        if (!cancelled) setIsLoadingFolders(false);
      });

    return () => {
      cancelled = true;
    };
  }, [selectedCollection]);

  if (tab.collectionName === null) {
    return (
      <div className='flex flex-col items-center justify-center h-full gap-3 p-6'>
        <p className='text-sm text-muted-foreground'>Choose a collection or folder to run</p>
        <Select value={selectedCollection} onValueChange={setSelectedCollection}>
          <SelectTrigger className='w-64' aria-label='Collection'>
            <SelectValue placeholder='Select collection' />
          </SelectTrigger>
          <SelectContent>
            {collections.map((collection) => (
              <SelectItem key={collection.name} value={collection.name}>
                {collection.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        {selectedCollection ? (
          <Select value={selectedFolder} onValueChange={setSelectedFolder}>
            <SelectTrigger className='w-64' aria-label='Folder' disabled={isLoadingFolders}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={WHOLE_COLLECTION}>Whole collection</SelectItem>
              {folders.map((folder) => (
                <SelectItem key={folder.path} value={folder.path}>
                  {folder.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        ) : null}
        <Button
          size='sm'
          disabled={!selectedCollection || isLoadingFolders}
          onClick={() => {
            const folderPath = selectedFolder === WHOLE_COLLECTION ? undefined : selectedFolder;
            void openRunnerTab(selectedCollection, folderPath);
            closeTab(tab.id, groupId);
          }}
        >
          Load
        </Button>
      </div>
    );
  }

  return (
    <div className='flex flex-col h-full'>
      <RunnerSummaryHeader tab={tab} />
      {tab.runState === 'idle' ? <RunnerRequestList tab={tab} /> : <RunnerResultsList tab={tab} />}
    </div>
  );
}
