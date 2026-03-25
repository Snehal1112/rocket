import { Plus } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { usePaneStore } from '@/stores/pane-store';
import { TabBar } from './TabBar';
import { RequestPanel } from '@/components/request/RequestPanel';
import { CollectionOverviewTab } from '@/components/collections/CollectionOverviewTab';
import type { LeafNode } from '@/types/pane-types';
import { isRequestTab } from '@/types/pane-types';

// Branded empty state shown when no tabs are open.
function EmptyState({ groupId }: { groupId: string }) {
  const newDraftTab = usePaneStore((s) => s.newDraftTab);

  return (
    <div className="flex h-full items-center justify-center">
      <div className="flex flex-col items-center gap-6 text-center max-w-sm">
        {/* Logo and branding. */}
        <div className="flex flex-col items-center gap-3">
          <img src="/rocket.png" alt="Rocket API" className="w-16 h-16 object-contain" />
          <div>
            <h2 className="text-lg font-semibold text-foreground">Rocket API</h2>
            <p className="text-sm text-muted-foreground mt-1">
              Fast, native API client for testing HTTP APIs.
            </p>
          </div>
        </div>

        {/* Quick actions. */}
        <div className="flex flex-col gap-2 w-full">
          <Button
            variant="default"
            className="w-full gap-2"
            onClick={() => newDraftTab(groupId)}
          >
            <Plus className="h-4 w-4" />
            New Request
          </Button>
          <p className="text-xs text-muted-foreground">
            or open a request from the collections sidebar
          </p>
        </div>

        {/* Keyboard shortcut hints. */}
        <div className="flex gap-4 text-xs text-muted-foreground">
          <span>
            <kbd className="rounded border border-border bg-muted px-1.5 py-0.5 font-mono text-[10px]">Cmd+T</kbd>
            {' '}New tab
          </span>
          <span>
            <kbd className="rounded border border-border bg-muted px-1.5 py-0.5 font-mono text-[10px]">Cmd+Enter</kbd>
            {' '}Send
          </span>
        </div>
      </div>
    </div>
  );
}

// Renders a tab bar at the top and the active tab content below.
export function EditorGroup({ node }: { node: LeafNode }) {
  const activeTab = node.tabs.find((t) => t.id === node.activeTabId);
  const hasTabs = node.tabs.length > 0;

  return (
    <div className="flex flex-col h-full">
      {hasTabs && <TabBar node={node} />}
      <div className="flex-1 overflow-hidden">
        {activeTab ? (
          isRequestTab(activeTab) ? (
            <RequestPanel tab={activeTab} groupId={node.groupId} />
          ) : (
            <CollectionOverviewTab collectionName={activeTab.collectionName} />
          )
        ) : (
          <EmptyState groupId={node.groupId} />
        )}
      </div>
    </div>
  );
}
