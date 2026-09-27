import { useEffect, useState } from 'react';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { type CollectionSummary, listCollections, listFlows } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { FlowTab } from '@/types/pane-types';

export function FlowPane({ tab, groupId }: { tab: FlowTab; groupId: string }) {
  const openFlowTab = usePaneStore((s) => s.openFlowTab);
  const closeTab = usePaneStore((s) => s.closeTab);
  const [collections, setCollections] = useState<CollectionSummary[]>([]);
  const [selectedCollection, setSelectedCollection] = useState('');
  const [flowNames, setFlowNames] = useState<string[]>([]);

  useEffect(() => {
    if (tab.flowName === null) {
      void listCollections().then(setCollections);
    }
  }, [tab.flowName]);

  useEffect(() => {
    if (!selectedCollection) {
      setFlowNames([]);
      return;
    }
    void listFlows(selectedCollection)
      .then(setFlowNames)
      .catch((err) => console.error('[FlowPane] failed to list flows', err));
  }, [selectedCollection]);

  if (tab.flowName === null) {
    return (
      <div className='flex flex-col items-center justify-center h-full gap-3 p-6'>
        <p className='text-sm text-muted-foreground'>Choose a collection and a flow</p>
        <Select value={selectedCollection} onValueChange={setSelectedCollection}>
          <SelectTrigger className='w-64' aria-label='Collection'>
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
        <Select
          disabled={!selectedCollection}
          onValueChange={(name) => {
            closeTab(tab.id, groupId);
            void openFlowTab(selectedCollection, name);
          }}
        >
          <SelectTrigger className='w-64' aria-label='Flow'>
            <SelectValue placeholder='Select flow' />
          </SelectTrigger>
          <SelectContent>
            {flowNames.map((name) => (
              <SelectItem key={name} value={name}>
                {name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
    );
  }

  // Real canvas rendering (React Flow) arrives in Plan 09 — this stub
  // confirms the tab/data plumbing works end to end first.
  return (
    <div className='flex h-full items-center justify-center text-sm text-muted-foreground'>
      Loaded flow &quot;{tab.flowName}&quot; with {tab.nodes.length} node(s) — canvas rendering
      lands in Plan 09.
    </div>
  );
}
