import {
  BookmarkPlus,
  Braces,
  Columns2,
  Globe,
  LayoutPanelLeft,
  PanelBottom,
  PanelRight,
  Pencil,
  Plus,
  Radio,
  Save,
  X,
  Zap,
} from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import { Input } from '@/components/ui/input';
import { usePaneStore } from '@/stores/pane-store';
import type { LeafNode, PaneNode } from '@/types/pane-types';
import { isWorkspaceTab } from '@/types/pane-types';
import { TabItem } from './TabItem';

// Collect all leaf groupIds from a pane tree recursively.
function collectLeafGroupIds(node: PaneNode): string[] {
  if (node.type === 'leaf') return [node.groupId];
  return [...collectLeafGroupIds(node.children[0]), ...collectLeafGroupIds(node.children[1])];
}

// Request tab bar matching legacy RequestTabs styling.
export function TabBar({
  node,
  onCloseTab,
}: {
  node: LeafNode;
  onCloseTab?: (tabId: string) => void;
}) {
  const setActiveTab = usePaneStore((s) => s.setActiveTab);
  const closeTab = usePaneStore((s) => s.closeTab);
  const splitGroup = usePaneStore((s) => s.splitGroup);
  const moveTab = usePaneStore((s) => s.moveTab);
  const root = usePaneStore((s) => s.root);
  const updateTabTitle = usePaneStore((s) => s.updateTabTitle);
  const openEphemeralTab = usePaneStore((s) => s.openEphemeralTab);

  // Other panes available for moving tabs into.
  const otherGroupIds = collectLeafGroupIds(root).filter((id) => id !== node.groupId);

  const [renamingTabId, setRenamingTabId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState('');
  const [isDragOver, setIsDragOver] = useState(false);

  function commitRename(tabId: string, fallback: string) {
    if (renamingTabId !== tabId) return;
    const newTitle = renameValue.trim() || fallback;
    setRenamingTabId(null);
    updateTabTitle(tabId, newTitle);
  }

  function handleDrop(e: React.DragEvent<HTMLDivElement>) {
    e.preventDefault();
    setIsDragOver(false);
    try {
      const { tabId, fromGroupId } = JSON.parse(e.dataTransfer.getData('text/plain')) as {
        tabId: string;
        fromGroupId: string;
      };
      // Skip if dropped onto the same pane.
      if (fromGroupId === node.groupId) return;
      usePaneStore.getState().moveTab(tabId, fromGroupId, node.groupId);
    } catch {
      // Ignore malformed drag data from other sources.
    }
  }

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: drop target for tab drag-and-drop
    <div
      className={`flex items-center border-b border-border/70 bg-card/70 backdrop-blur-sm overflow-x-auto overflow-y-hidden shrink-0 ${isDragOver ? 'ring-2 ring-primary/60 ring-inset' : ''}`}
      onDragOver={(e) => {
        e.preventDefault();
        e.dataTransfer.dropEffect = 'move';
        setIsDragOver(true);
      }}
      onDragLeave={() => setIsDragOver(false)}
      onDrop={handleDrop}
    >
      {node.tabs.map((tab) => (
        <ContextMenu key={tab.id}>
          <ContextMenuTrigger asChild>
            <div>
              {renamingTabId === tab.id ? (
                // Inline rename input replacing the tab item.
                <div className='flex items-center px-2 py-1 border-r border-border/70 bg-background/95 border-b-2 border-b-primary -mb-px shrink-0'>
                  <Input
                    autoFocus
                    className='h-5 w-28 text-xs px-1'
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
                  fromGroupId={node.groupId}
                  onSelect={() => setActiveTab(tab.id, node.groupId)}
                  onClose={() => (onCloseTab ? onCloseTab(tab.id) : closeTab(tab.id, node.groupId))}
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
                window.dispatchEvent(
                  new CustomEvent('rocket:save-draft', {
                    detail: { tabId: tab.id },
                  }),
                );
              }}
            >
              {tab.source ? (
                <>
                  <Save className='h-3.5 w-3.5 mr-2' /> Save
                </>
              ) : (
                <>
                  <BookmarkPlus className='h-3.5 w-3.5 mr-2' /> Save to collection...
                </>
              )}
            </ContextMenuItem>
            <ContextMenuItem
              onClick={() => {
                setRenamingTabId(tab.id);
                setRenameValue(tab.title);
              }}
            >
              <Pencil className='h-3.5 w-3.5 mr-2' /> Rename
            </ContextMenuItem>
            <ContextMenuSeparator />
            <ContextMenuItem
              disabled={isWorkspaceTab(tab)}
              onClick={() => (onCloseTab ? onCloseTab(tab.id) : closeTab(tab.id, node.groupId))}
            >
              <X className='h-3.5 w-3.5 mr-2' /> Close
            </ContextMenuItem>
            <ContextMenuItem
              disabled={isWorkspaceTab(tab)}
              onClick={() => {
                node.tabs
                  .filter((t) => t.id !== tab.id)
                  .forEach((t) => {
                    if (onCloseTab) onCloseTab(t.id);
                    else closeTab(t.id, node.groupId);
                  });
              }}
            >
              <Columns2 className='h-3.5 w-3.5 mr-2' /> Close Others
            </ContextMenuItem>
            <ContextMenuSeparator />
            <ContextMenuItem onClick={() => splitGroup(node.groupId, 'horizontal')}>
              <PanelRight className='h-3.5 w-3.5 mr-2' /> Split Right
            </ContextMenuItem>
            <ContextMenuItem onClick={() => splitGroup(node.groupId, 'vertical')}>
              <PanelBottom className='h-3.5 w-3.5 mr-2' /> Split Down
            </ContextMenuItem>
            <ContextMenuSeparator />
            {otherGroupIds.length === 1 && (
              <ContextMenuItem onClick={() => moveTab(tab.id, node.groupId, otherGroupIds[0])}>
                <LayoutPanelLeft className='h-3.5 w-3.5 mr-2' /> Move to other pane
              </ContextMenuItem>
            )}
            {otherGroupIds.length > 1 && (
              <ContextMenuSub>
                <ContextMenuSubTrigger>
                  <LayoutPanelLeft className='h-3.5 w-3.5 mr-2' /> Move to other pane
                </ContextMenuSubTrigger>
                <ContextMenuSubContent>
                  {otherGroupIds.map((id, i) => (
                    <ContextMenuItem key={id} onClick={() => moveTab(tab.id, node.groupId, id)}>
                      Pane {i + 2}
                    </ContextMenuItem>
                  ))}
                </ContextMenuSubContent>
              </ContextMenuSub>
            )}
            <ContextMenuItem
              onClick={() => {
                const beforeIds = collectLeafGroupIds(usePaneStore.getState().root);
                splitGroup(node.groupId, 'horizontal');
                const afterIds = collectLeafGroupIds(usePaneStore.getState().root);
                const newId = afterIds.find((id) => !beforeIds.includes(id));
                if (newId) moveTab(tab.id, node.groupId, newId);
              }}
            >
              <PanelRight className='h-3.5 w-3.5 mr-2' /> Move to new split right
            </ContextMenuItem>
            <ContextMenuItem
              onClick={() => {
                const beforeIds = collectLeafGroupIds(usePaneStore.getState().root);
                splitGroup(node.groupId, 'vertical');
                const afterIds = collectLeafGroupIds(usePaneStore.getState().root);
                const newId = afterIds.find((id) => !beforeIds.includes(id));
                if (newId) moveTab(tab.id, node.groupId, newId);
              }}
            >
              <PanelBottom className='h-3.5 w-3.5 mr-2' /> Move to new split below
            </ContextMenuItem>
          </ContextMenuContent>
        </ContextMenu>
      ))}
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <Button
            type='button'
            variant='ghost'
            size='icon'
            aria-label='New request'
            title='New request - Right-click for more types'
            onClick={() => openEphemeralTab('http')}
            className='ml-1 h-7 w-7 shrink-0 text-muted-foreground hover:text-foreground'
          >
            <Plus className='h-3.5 w-3.5' />
          </Button>
        </ContextMenuTrigger>
        <ContextMenuContent>
          <ContextMenuItem onClick={() => openEphemeralTab('http')}>
            <Globe className='h-3.5 w-3.5 mr-2' /> HTTP
          </ContextMenuItem>
          <ContextMenuItem onClick={() => openEphemeralTab('graphql')}>
            <Braces className='h-3.5 w-3.5 mr-2' /> GraphQL
          </ContextMenuItem>
          <ContextMenuItem onClick={() => openEphemeralTab('grpc')}>
            <Zap className='h-3.5 w-3.5 mr-2' /> gRPC
          </ContextMenuItem>
          <ContextMenuItem onClick={() => openEphemeralTab('websocket')}>
            <Radio className='h-3.5 w-3.5 mr-2' /> WebSocket
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
    </div>
  );
}
