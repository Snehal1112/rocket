import { useState } from 'react';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import { Input } from '@/components/ui/input';
import { PanelRight, PanelBottom } from 'lucide-react';
import { TabItem } from './TabItem';
import { usePaneStore } from '@/stores/pane-store';
import type { LeafNode } from '@/types/pane-types';

// Request tab bar matching legacy RequestTabs styling.
export function TabBar({ node, onCloseTab }: { node: LeafNode; onCloseTab?: (tabId: string) => void }) {
  const setActiveTab = usePaneStore((s) => s.setActiveTab);
  const closeTab = usePaneStore((s) => s.closeTab);

  const splitGroup = usePaneStore((s) => s.splitGroup);
  const updateTabTitle = usePaneStore((s) => s.updateTabTitle);

  const [renamingTabId, setRenamingTabId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState('');

  function commitRename(tabId: string, fallback: string) {
    if (renamingTabId !== tabId) return;
    const newTitle = renameValue.trim() || fallback;
    setRenamingTabId(null);
    updateTabTitle(tabId, newTitle);
  }

  return (
    <div className="flex items-center border-b border-border/70 bg-card/70 backdrop-blur-sm overflow-x-auto overflow-y-hidden shrink-0">
      {node.tabs.map((tab) => (
        <ContextMenu key={tab.id}>
          <ContextMenuTrigger asChild>
            <div>
              {renamingTabId === tab.id ? (
                // Inline rename input replacing the tab item.
                <div className="flex items-center px-2 py-1 border-r border-border/70 bg-background/95 border-b-2 border-b-primary -mb-px shrink-0">
                  <Input
                    autoFocus
                    className="h-5 w-28 text-xs px-1"
                    value={renameValue}
                    onChange={(e) => setRenameValue(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') commitRename(tab.id, tab.title);
                      if (e.key === 'Escape') setRenamingTabId(null);
                    }}
                    onBlur={() => commitRename(tab.id, tab.title)}
                    onClick={(e) => e.stopPropagation()}
                  />
                </div>
              ) : (
                <TabItem
                  tab={tab}
                  isActive={tab.id === node.activeTabId}
                  onSelect={() => setActiveTab(tab.id, node.groupId)}
                  onClose={() => onCloseTab ? onCloseTab(tab.id) : closeTab(tab.id, node.groupId)}
                  onDoubleClick={() => {
                    setRenamingTabId(tab.id);
                    setRenameValue(tab.title);
                  }}
                />
              )}
            </div>
          </ContextMenuTrigger>
          <ContextMenuContent>
            <ContextMenuItem
              disabled={!tab.isDirty && !!tab.source}
              onClick={() => {
                window.dispatchEvent(new CustomEvent('rocket:save-draft', { detail: { tabId: tab.id } }));
              }}
            >
              {tab.source ? 'Save' : 'Save to collection...'}
            </ContextMenuItem>
            <ContextMenuItem
              onClick={() => {
                setRenamingTabId(tab.id);
                setRenameValue(tab.title);
              }}
            >
              Rename
            </ContextMenuItem>
            <ContextMenuSeparator />
            <ContextMenuItem onClick={() => onCloseTab ? onCloseTab(tab.id) : closeTab(tab.id, node.groupId)}>
              Close
            </ContextMenuItem>
            <ContextMenuItem
              onClick={() => {
                node.tabs
                  .filter((t) => t.id !== tab.id)
                  .forEach((t) => onCloseTab ? onCloseTab(t.id) : closeTab(t.id, node.groupId));
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
    </div>
  );
}
