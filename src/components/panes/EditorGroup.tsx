import { useState } from 'react';
import { CollectionOverviewTab } from '@/components/collections/CollectionOverviewTab';
import { ContractTab } from '@/components/contract/ContractTab';
import { ConflictResolver } from '@/components/git/ConflictResolver';
import { DiffViewer } from '@/components/git/DiffViewer';
import { GitPanel } from '@/components/git/GitPanel';
import { RocketLaunch } from '@/components/illustrations';
import { RequestPanel } from '@/components/request/RequestPanel';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { WorkspaceEnvironmentsTab } from '@/components/workspace/WorkspaceEnvironmentsTab';
import { WorkspaceGitTab } from '@/components/workspace/WorkspaceGitTab';
import { WorkspaceOverviewTab } from '@/components/workspace/WorkspaceOverviewTab';
import { usePaneStore } from '@/stores/pane-store';
import type { LeafNode } from '@/types/pane-types';
import {
  isConflictTab,
  isContractTab,
  isDiffTab,
  isGitTab,
  isRequestTab,
  isWorkspaceTab,
} from '@/types/pane-types';
import { TabBar } from './TabBar';

// Branded empty state shown when no tabs are open.
function EmptyState() {
  return (
    <div className='flex h-full items-center justify-center'>
      <div className='flex flex-col items-center gap-6 text-center max-w-sm'>
        <div className='flex flex-col items-center gap-3'>
          <RocketLaunch className='w-32 h-32' />
          <div>
            <h2 className='text-lg font-semibold text-foreground'>Rocket API</h2>
            <p className='text-sm text-muted-foreground mt-1'>
              Open a request from the collections sidebar to get started.
            </p>
          </div>
        </div>
        <div className='flex gap-4 text-xs text-muted-foreground'>
          <span>
            <kbd className='rounded border border-border bg-muted px-1.5 py-0.5 font-mono text-2xs'>
              Cmd+Enter
            </kbd>{' '}
            Send
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

  const closeTab = usePaneStore((s) => s.closeTab);
  const [pendingCloseTabId, setPendingCloseTabId] = useState<string | null>(null);

  const handleCloseTab = (tabId: string) => {
    const tab = node.tabs.find((t) => t.id === tabId);
    // Guard only if the tab has unsaved edits.
    const needsGuard = tab?.isDirty ?? false;
    if (tab && needsGuard) {
      setPendingCloseTabId(tabId);
    } else {
      closeTab(tabId, node.groupId);
    }
  };

  return (
    <div className='flex flex-col h-full'>
      {hasTabs && <TabBar node={node} onCloseTab={handleCloseTab} />}
      <div className='flex-1 overflow-hidden'>
        {activeTab ? (
          isConflictTab(activeTab) ? (
            <ConflictResolver conflictState={activeTab.conflictState} />
          ) : isDiffTab(activeTab) ? (
            <DiffViewer diffState={activeTab.diffState} />
          ) : isRequestTab(activeTab) ? (
            <RequestPanel tab={activeTab} groupId={node.groupId} />
          ) : isGitTab(activeTab) ? (
            <GitPanel
              collectionPath={activeTab.collectionPath}
              collectionName={activeTab.collectionName}
            />
          ) : isContractTab(activeTab) ? (
            <ContractTab tab={activeTab} />
          ) : isWorkspaceTab(activeTab) ? (
            activeTab.activeSection === 'overview' ? (
              <WorkspaceOverviewTab workspaceId={activeTab.workspaceId} />
            ) : activeTab.activeSection === 'environments' ? (
              <WorkspaceEnvironmentsTab />
            ) : activeTab.activeSection === 'git' ? (
              <WorkspaceGitTab workspaceId={activeTab.workspaceId} />
            ) : null
          ) : (
            <CollectionOverviewTab tab={activeTab} />
          )
        ) : (
          <EmptyState />
        )}
      </div>
      <AlertDialog
        open={!!pendingCloseTabId}
        onOpenChange={(open) => {
          if (!open) setPendingCloseTabId(null);
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Unsaved Changes</AlertDialogTitle>
            <AlertDialogDescription>
              {(() => {
                if (!pendingCloseTabId) return null;
                const found = node.tabs.find((t) => t.id === pendingCloseTabId);
                if (found && isRequestTab(found) && !found.source) {
                  return 'This request has never been saved to a collection. Closing it will discard all changes. Close anyway?';
                }
                return 'This request has unsaved changes. Close anyway?';
              })()}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                if (pendingCloseTabId) closeTab(pendingCloseTabId, node.groupId);
                setPendingCloseTabId(null);
              }}
            >
              Close
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
