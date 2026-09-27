import { Plus } from 'lucide-react';
import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { type CollectionSummary, listCollections, listFlows, saveFlow } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { FlowTab } from '@/types/pane-types';
import { FlowCanvas } from './FlowCanvas';

export function FlowPane({ tab, groupId }: { tab: FlowTab; groupId: string }) {
  const openFlowTab = usePaneStore((s) => s.openFlowTab);
  const closeTab = usePaneStore((s) => s.closeTab);
  const updateFlowNodes = usePaneStore((s) => s.updateFlowNodes);
  const updateFlowEdges = usePaneStore((s) => s.updateFlowEdges);
  const [collections, setCollections] = useState<CollectionSummary[]>([]);
  // Start from the tab's own collection, so a tab opened for a collection
  // (or one that fell back after a failed load) keeps that choice.
  const [selectedCollection, setSelectedCollection] = useState(tab.collectionName ?? '');
  const [flowNames, setFlowNames] = useState<string[]>([]);
  const [newFlowName, setNewFlowName] = useState('');
  const [isCreating, setIsCreating] = useState(false);

  useEffect(() => {
    if (tab.flowName === null) {
      void listCollections()
        .then(setCollections)
        .catch((err) => console.error('[FlowPane] failed to list collections', err));
    }
  }, [tab.flowName]);

  useEffect(() => {
    if (!selectedCollection) {
      setFlowNames([]);
      return;
    }
    // Ignore a stale response when the user switches collections quickly.
    let cancelled = false;
    setFlowNames([]);
    void listFlows(selectedCollection)
      .then((names) => {
        if (!cancelled) setFlowNames(names);
      })
      .catch((err) => console.error('[FlowPane] failed to list flows', err));
    return () => {
      cancelled = true;
    };
  }, [selectedCollection]);

  const openFlow = (name: string) => {
    closeTab(tab.id, groupId);
    void openFlowTab(selectedCollection, name);
  };

  // Saves an empty flow first, so the new tab loads it like any existing one.
  const handleCreate = async () => {
    const name = newFlowName.trim();
    if (!selectedCollection || !name) return;
    if (flowNames.includes(name)) {
      openFlow(name);
      return;
    }
    setIsCreating(true);
    try {
      await saveFlow(selectedCollection, { name, nodes: [], edges: [] });
      openFlow(name);
    } catch (err) {
      toast.error(`Could not create flow: ${String(err)}`);
    } finally {
      setIsCreating(false);
    }
  };

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
        <Select disabled={!selectedCollection || flowNames.length === 0} onValueChange={openFlow}>
          <SelectTrigger className='w-64' aria-label='Flow'>
            <SelectValue placeholder={flowNames.length === 0 ? 'No flows yet' : 'Select flow'} />
          </SelectTrigger>
          <SelectContent>
            {flowNames.map((name) => (
              <SelectItem key={name} value={name}>
                {name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <div className='flex w-64 items-center gap-2'>
          <Input
            aria-label='New flow name'
            placeholder='New flow name'
            value={newFlowName}
            disabled={!selectedCollection || isCreating}
            onChange={(e) => setNewFlowName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void handleCreate();
            }}
          />
          <Button
            size='sm'
            variant='outline'
            aria-label='Create flow'
            disabled={!selectedCollection || !newFlowName.trim() || isCreating}
            onClick={() => void handleCreate()}
          >
            <Plus className='h-3.5 w-3.5' />
          </Button>
        </div>
      </div>
    );
  }

  return (
    <FlowCanvas
      nodes={tab.nodes}
      edges={tab.edges}
      nodeStatus={tab.nodeStatus}
      onNodesChange={(nodes) => updateFlowNodes(tab.id, nodes)}
      onEdgesChange={(edges) => updateFlowEdges(tab.id, edges)}
      onConnect={() => {
        /* Plan 10 replaces this with real edge-creation + the expression editor popover */
      }}
    />
  );
}
