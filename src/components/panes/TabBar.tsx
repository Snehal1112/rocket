import type { LeafNode } from '@/types/pane-types';
import { usePaneStore } from '@/stores/pane-store';
import { Plus } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { TabItem } from './TabItem';

// Horizontal tab strip with a "+" button to create new draft tabs.
export function TabBar({ node }: { node: LeafNode }) {
  const newDraftTab = usePaneStore((s) => s.newDraftTab);

  return (
    <div className="flex items-center border-b border-border bg-muted/50">
      <div className="flex flex-1 items-center overflow-x-auto">
        {node.tabs.map((tab) => (
          <TabItem
            key={tab.id}
            tab={tab}
            groupId={node.groupId}
            isActive={tab.id === node.activeTabId}
          />
        ))}
      </div>
      <Button
        variant="ghost"
        size="icon"
        className="mx-1 size-7 shrink-0"
        onClick={() => newDraftTab(node.groupId)}
        aria-label="New tab"
      >
        <Plus className="size-3.5" />
      </Button>
    </div>
  );
}
