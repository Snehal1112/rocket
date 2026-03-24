import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import { Button } from '@/components/ui/button';
import { ScrollArea, ScrollBar } from '@/components/ui/scroll-area';
import { Plus, PanelRight, PanelBottom } from 'lucide-react';
import { TabItem } from './TabItem';
import { usePaneStore } from '@/stores/pane-store';
import type { LeafNode } from '@/types/pane-types';

// Horizontal tab strip with context menus, scroll support, and split controls.
export function TabBar({ node }: { node: LeafNode }) {
  const setActiveTab = usePaneStore((s) => s.setActiveTab);
  const closeTab = usePaneStore((s) => s.closeTab);
  const newDraftTab = usePaneStore((s) => s.newDraftTab);
  const splitGroup = usePaneStore((s) => s.splitGroup);

  return (
    <div className="flex items-center border-b border-border bg-muted/50">
      <ScrollArea className="flex-1">
        <div className="flex" role="tablist">
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
                <ContextMenuItem
                  onClick={() => splitGroup(node.groupId, 'horizontal')}
                >
                  <PanelRight className="size-4 mr-2" /> Split Right
                </ContextMenuItem>
                <ContextMenuItem
                  onClick={() => splitGroup(node.groupId, 'vertical')}
                >
                  <PanelBottom className="size-4 mr-2" /> Split Down
                </ContextMenuItem>
              </ContextMenuContent>
            </ContextMenu>
          ))}
        </div>
        <ScrollBar orientation="horizontal" />
      </ScrollArea>
      <div className="flex items-center gap-0.5 mx-1 shrink-0">
        <Button
          variant="ghost"
          size="icon"
          className="size-7"
          onClick={() => newDraftTab(node.groupId)}
          aria-label="New tab"
        >
          <Plus className="size-3.5" />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="size-7"
          onClick={() => splitGroup(node.groupId, 'horizontal')}
          aria-label="Split right"
        >
          <PanelRight className="size-3.5" />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="size-7"
          onClick={() => splitGroup(node.groupId, 'vertical')}
          aria-label="Split down"
        >
          <PanelBottom className="size-3.5" />
        </Button>
      </div>
    </div>
  );
}
