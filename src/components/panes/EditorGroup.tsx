import { lazy, Suspense, useState } from 'react';
import { AuditLogTab } from '@/components/audit/AuditLogTab';
import { CollectionOverviewTab } from '@/components/collections/CollectionOverviewTab';
import { ContractTab } from '@/components/contract/ContractTab';
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';

// Lazy-load Monaco-heavy git components so they don't load until a git tab opens.
const ConflictResolver = lazy(() =>
  import('@/components/git/ConflictResolver').then((m) => ({ default: m.ConflictResolver })),
);
const DiffViewer = lazy(() =>
  import('@/components/git/DiffViewer').then((m) => ({ default: m.DiffViewer })),
);

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
    <div className='flex h-full items-center justify-center bg-gradient-to-b from-background to-muted/20'>
      <div className='flex flex-col items-center gap-8 text-center max-w-xs px-6'>
        <RocketLaunch className='w-36 h-36 drop-shadow-sm' />

        <div className='space-y-2'>
          <h2 className='text-base font-semibold tracking-tight text-foreground'>
            Ready for launch
          </h2>
          <p className='text-sm text-muted-foreground leading-relaxed'>
            Select a request from the sidebar, or create a new one to get started.
          </p>
        </div>

        <div className='flex items-center gap-3 text-xs text-muted-foreground/70'>
          <span className='flex items-center gap-1.5'>
            <kbd className='rounded border border-border/60 bg-muted/80 px-1.5 py-0.5 font-mono text-2xs text-muted-foreground'>
              ⌘ Enter
            </kbd>
            Send request
          </span>
          <span className='w-px h-3 bg-border/60' />
          <span className='flex items-center gap-1.5'>
            <kbd className='rounded border border-border/60 bg-muted/80 px-1.5 py-0.5 font-mono text-2xs text-muted-foreground'>
              ⌘ N
            </kbd>
            New request
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
            <Suspense fallback={<EditorSkeleton />}>
              <ConflictResolver conflictState={activeTab.conflictState} />
            </Suspense>
          ) : isDiffTab(activeTab) ? (
            <Suspense fallback={<EditorSkeleton />}>
              <DiffViewer diffState={activeTab.diffState} />
            </Suspense>
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
            ) : activeTab.activeSection === 'audit' ? (
              <AuditLogTab />
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
