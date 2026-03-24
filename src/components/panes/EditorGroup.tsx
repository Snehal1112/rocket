import { TabBar } from './TabBar';
import { RequestPanel } from '@/components/request/RequestPanel';
import { CollectionOverviewTab } from '@/components/collections/CollectionOverviewTab';
import type { LeafNode } from '@/types/pane-types';
import { isRequestTab } from '@/types/pane-types';

// Renders a tab bar at the top and the active tab content below.
export function EditorGroup({ node }: { node: LeafNode }) {
  const activeTab = node.tabs.find((t) => t.id === node.activeTabId);

  return (
    <div className="flex flex-col h-full">
      <TabBar node={node} />
      <div className="flex-1 overflow-hidden">
        {activeTab ? (
          isRequestTab(activeTab) ? (
            <RequestPanel tab={activeTab} groupId={node.groupId} />
          ) : (
            <CollectionOverviewTab collectionName={activeTab.collectionName} />
          )
        ) : (
          <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
            No open tabs
          </div>
        )}
      </div>
    </div>
  );
}
