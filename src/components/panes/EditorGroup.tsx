import type { LeafNode } from '@/types/pane-types';
import { TabBar } from './TabBar';

// Renders a tab bar at the top and the active tab content below.
export function EditorGroup({ node }: { node: LeafNode }) {
  const activeTab = node.tabs.find((t) => t.id === node.activeTabId);

  return (
    <div className="flex h-full flex-col">
      <TabBar node={node} />
      <div className="flex-1 overflow-auto p-4">
        {activeTab ? (
          <div>
            <span className="text-sm text-muted-foreground">
              {activeTab.request.method} {activeTab.request.url || 'New request'}
            </span>
          </div>
        ) : (
          <div className="text-muted-foreground">No active tab</div>
        )}
      </div>
    </div>
  );
}
