import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { type CollectionSummary, listCollections } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { RunnerTab } from '@/types/pane-types';
import { RunnerRequestList } from './RunnerRequestList';
import { RunnerResultsList } from './RunnerResultsList';
import { RunnerSummaryHeader } from './RunnerSummaryHeader';

export function RunnerPane({ tab, groupId }: { tab: RunnerTab; groupId: string }) {
  const openRunnerTab = usePaneStore((s) => s.openRunnerTab);
  const closeTab = usePaneStore((s) => s.closeTab);
  const [collections, setCollections] = useState<CollectionSummary[]>([]);
  const [selected, setSelected] = useState('');

  useEffect(() => {
    if (tab.collectionName === null) {
      void listCollections().then(setCollections);
    }
  }, [tab.collectionName]);

  if (tab.collectionName === null) {
    return (
      <div className='flex flex-col items-center justify-center h-full gap-3 p-6'>
        <p className='text-sm text-muted-foreground'>Choose a collection to run</p>
        <Select value={selected} onValueChange={setSelected}>
          <SelectTrigger className='w-64'>
            <SelectValue placeholder='Select collection' />
          </SelectTrigger>
          <SelectContent>
            {collections.map((c) => (
              <SelectItem key={c.name} value={c.name}>
                {c.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button
          size='sm'
          disabled={!selected}
          onClick={() => {
            void openRunnerTab(selected);
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
