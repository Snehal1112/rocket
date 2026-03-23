import type { LeafNode } from '@/types/pane-types';
import { TabBar } from './TabBar';
import { RequestPanel } from '@/components/request/RequestPanel';

// Renders a tab bar at the top and the active tab content below.
export function EditorGroup({ node }: { node: LeafNode }) {
  const activeTab = node.tabs.find((t) => t.id === node.activeTabId);

  return (
    <div className="flex h-full flex-col">
      <TabBar node={node} />
      <div className="flex-1 overflow-hidden">
        {activeTab ? (
          <RequestPanel tab={activeTab} groupId={node.groupId} />
        ) : (
          <div className="flex h-full items-center justify-center text-muted-foreground">
            No active tab
          </div>
        )}
      </div>
    </div>
  );
}
