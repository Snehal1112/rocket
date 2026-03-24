import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import { Button } from '@/components/ui/button';
import { Plus, PanelRight, PanelBottom } from 'lucide-react';
import { TabItem } from './TabItem';
import { usePaneStore } from '@/stores/pane-store';
import type { LeafNode } from '@/types/pane-types';

// Request tab bar matching legacy RequestTabs styling.
export function TabBar({ node }: { node: LeafNode }) {
  const setActiveTab = usePaneStore((s) => s.setActiveTab);
  const closeTab = usePaneStore((s) => s.closeTab);
  const newDraftTab = usePaneStore((s) => s.newDraftTab);
  const splitGroup = usePaneStore((s) => s.splitGroup);

  return (
    <div className="flex items-center border-b border-border/70 bg-card/70 backdrop-blur-sm overflow-x-auto overflow-y-hidden shrink-0">
      {node.tabs.map((tab) => (
        <ContextMenu key={tab.id}>
          <ContextMenuTrigger asChild>
            <div>
              <TabItem
                tab={tab}
                isActive={tab.id === node.activeTabId}
                onSelect={() => setActiveTab(tab.id, node.groupId)}
                onClose={() => closeTab(tab.id, node.groupId)}
              />
            </div>
          </ContextMenuTrigger>
          <ContextMenuContent>
            <ContextMenuItem onClick={() => closeTab(tab.id, node.groupId)}>
              Close
            </ContextMenuItem>
            <ContextMenuItem
              onClick={() => {
                node.tabs
                  .filter((t) => t.id !== tab.id)
                  .forEach((t) => closeTab(t.id, node.groupId));
              }}
            >
              Close Others
            </ContextMenuItem>
            <ContextMenuSeparator />
            <ContextMenuItem onClick={() => splitGroup(node.groupId, 'horizontal')}>
              <PanelRight className="size-4 mr-2" /> Split Right
            </ContextMenuItem>
            <ContextMenuItem onClick={() => splitGroup(node.groupId, 'vertical')}>
              <PanelBottom className="size-4 mr-2" /> Split Down
            </ContextMenuItem>
          </ContextMenuContent>
        </ContextMenu>
      ))}

      <Button
        variant="ghost"
        size="icon"
        className="h-8 w-8 shrink-0 rounded-none hover:bg-accent/60"
        onClick={() => newDraftTab(node.groupId)}
        aria-label="New tab"
      >
        <Plus className="h-3.5 w-3.5" />
      </Button>
    </div>
  );
}
