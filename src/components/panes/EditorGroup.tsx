import { lazy, Suspense, useState } from 'react';
import { CollectionOverviewTab } from '@/components/collections/CollectionOverviewTab';
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';
import { WebSocketPanel } from '@/components/request/websocket/WebSocketPanel';

// Lazy-load tabs that aren't shown on a default/cold launch, so their code
// (and anything heavy they pull in, e.g. GitPanel's diff libs or RunnerPane's
// recharts) isn't parsed and resident until the user actually opens one.
const ConflictResolver = lazy(() =>
  import('@/components/git/ConflictResolver').then((m) => ({ default: m.ConflictResolver })),
);
const DiffViewer = lazy(() =>
  import('@/components/git/DiffViewer').then((m) => ({ default: m.DiffViewer })),
);
const GitPanel = lazy(() =>
  import('@/components/git/GitPanel').then((m) => ({ default: m.GitPanel })),
);
const ContractsTab = lazy(() =>
  import('@/components/contracts/ContractsTab').then((m) => ({ default: m.ContractsTab })),
);
const ScriptFilePane = lazy(() =>
  import('@/components/scripts/ScriptFilePane').then((m) => ({ default: m.ScriptFilePane })),
);

const ContractDiffPane = lazy(() =>
  import('@/components/contracts/ContractDiffPane').then((m) => ({ default: m.ContractDiffPane })),
);
const AuditLogTab = lazy(() =>
  import('@/components/audit/AuditLogTab').then((m) => ({ default: m.AuditLogTab })),
);
const RunnerPane = lazy(() =>
  import('@/components/request/runner/RunnerPane').then((m) => ({ default: m.RunnerPane })),
);
const FlowPane = lazy(() =>
  import('@/components/flow/FlowPane').then((m) => ({ default: m.FlowPane })),
);
const WorkspaceEnvironmentsTab = lazy(() =>
  import('@/components/workspace/WorkspaceEnvironmentsTab').then((m) => ({
    default: m.WorkspaceEnvironmentsTab,
  })),
);
const WorkspaceGitTab = lazy(() =>
  import('@/components/workspace/WorkspaceGitTab').then((m) => ({ default: m.WorkspaceGitTab })),
);

import { MousePointer2 } from 'lucide-react';
import { GrpcPanel } from '@/components/grpc/GrpcPanel';
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
import { WorkspaceOverviewTab } from '@/components/workspace/WorkspaceOverviewTab';
import { usePaneStore } from '@/stores/pane-store';
import type { LeafNode } from '@/types/pane-types';
import {
  isConflictTab,
  isContractDiffTab,
  isContractTab,
  isDiffTab,
  isFlowTab,
  isGitTab,
  isRequestTab,
  isRunnerTab,
  isScriptTab,
  isWorkspaceTab,
} from '@/types/pane-types';
import { BreadcrumbBar } from './BreadcrumbBar';
import { TabBar } from './TabBar';

type EmptyStateVariant = 'default' | 'active-split' | 'inactive-split';

interface EmptyStateProps {
  variant: EmptyStateVariant;
  onActivate?: () => void;
}

// Shows context-aware guidance when no tabs are open in a pane.
function EmptyState({ variant, onActivate }: EmptyStateProps) {
  if (variant === 'inactive-split') {
    return (
      <button
        type='button'
        className='flex h-full w-full items-center justify-center bg-gradient-to-b from-background to-muted/20 cursor-pointer border-0 p-0'
        onClick={onActivate}
      >
        <div className='flex flex-col items-center gap-4 text-center max-w-xs px-6'>
          <MousePointer2 className='w-8 h-8 text-muted-foreground/50' />
          <div className='space-y-1.5'>
            <h2 className='text-sm font-semibold tracking-tight text-foreground'>
              Pane not focused
            </h2>
            <p className='text-sm text-muted-foreground leading-relaxed'>
              Click to focus this pane, then select a request from the sidebar.
            </p>
          </div>
        </div>
      </button>
    );
  }

  if (variant === 'active-split') {
    return (
      <div className='flex h-full items-center justify-center bg-gradient-to-b from-background to-muted/20'>
        <div className='flex flex-col items-center gap-4 text-center max-w-xs px-6'>
          <div className='space-y-1.5'>
            <h2 className='text-sm font-semibold tracking-tight text-foreground'>Pane ready</h2>
            <p className='text-sm text-muted-foreground leading-relaxed'>
              Select a request from the sidebar to open it here.
            </p>
          </div>
        </div>
      </div>
    );
  }

  // Default single-pane branded state.
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
  const setActiveGroup = usePaneStore((s) => s.setActiveGroup);
  const activeGroupId = usePaneStore((s) => s.activeGroupId);
  const root = usePaneStore((s) => s.root);

  const isInSplitLayout = root.type === 'split';
  const isActive = activeGroupId === node.groupId;
  const isActivePaneInSplit = isInSplitLayout && isActive;
  const isInactivePaneInSplit = isInSplitLayout && !isActive;

  // Determine which empty-state variant to display.
  const emptyStateVariant: EmptyStateVariant = isInactivePaneInSplit
    ? 'inactive-split'
    : isActivePaneInSplit
      ? 'active-split'
      : 'default';

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
    // onMouseDown here is intentional UX — tracks which pane the user is clicking into.
    // biome-ignore lint/a11y/noStaticElementInteractions: pane focus tracking
    <section
      className={`flex flex-col h-full bg-card${isInSplitLayout && isActive ? ' ring-1 ring-primary/40' : ''}`}
      onMouseDown={() => setActiveGroup(node.groupId)}
    >
      {(hasTabs || isInSplitLayout) && <TabBar node={node} onCloseTab={handleCloseTab} />}
      {activeTab && <BreadcrumbBar tab={activeTab} />}
      <div className='flex-1 overflow-hidden'>
        {activeTab ? (
          isConflictTab(activeTab) ? (
            // `ConflictResolver` reads the git store via the per-`GitPanel`
            // React Context (`@/stores/git-store-context`) and throws
            // outside a `GitStoreProvider` ancestor. This render site has no
            // such ancestor. It is currently unreachable — nothing in this
            // codebase creates a conflict tab (`openConflictTab`, which used
            // to create one, was removed as dead code) — but if that ever
            // changes, this render will crash until it's wrapped in a
            // `GitStoreProvider` for the tab's own repository.
            <Suspense fallback={<EditorSkeleton />}>
              <ConflictResolver conflictState={activeTab.conflictState} />
            </Suspense>
          ) : isDiffTab(activeTab) ? (
            <Suspense fallback={<EditorSkeleton />}>
              <DiffViewer diffState={activeTab.diffState} />
            </Suspense>
          ) : isRequestTab(activeTab) ? (
            activeTab.request.requestType === 'websocket' ? (
              <WebSocketPanel tab={activeTab} groupId={node.groupId} />
            ) : activeTab.request.requestType === 'grpc' ? (
              <GrpcPanel tab={activeTab} groupId={node.groupId} />
            ) : (
              <RequestPanel tab={activeTab} groupId={node.groupId} />
            )
          ) : isGitTab(activeTab) ? (
            <Suspense fallback={<EditorSkeleton />}>
              <GitPanel
                key={activeTab.repositoryId}
                repositoryId={activeTab.repositoryId}
                repositoryLabel={activeTab.repositoryLabel}
              />
            </Suspense>
          ) : isContractTab(activeTab) ? (
            <Suspense fallback={<EditorSkeleton />}>
              <ContractsTab
                collectionId={activeTab.collectionRoot}
                collectionName={activeTab.collectionName}
              />
            </Suspense>
          ) : isContractDiffTab(activeTab) ? (
            <Suspense fallback={<EditorSkeleton />}>
              <ContractDiffPane
                collectionId={activeTab.collectionId}
                contractId={activeTab.contractId}
              />
            </Suspense>
          ) : isScriptTab(activeTab) ? (
            <Suspense fallback={<EditorSkeleton />}>
              <ScriptFilePane key={activeTab.id} tab={activeTab} />
            </Suspense>
          ) : isWorkspaceTab(activeTab) ? (
            activeTab.activeSection === 'overview' ? (
              <WorkspaceOverviewTab workspaceId={activeTab.workspaceId} />
            ) : activeTab.activeSection === 'environments' ? (
              <Suspense fallback={<EditorSkeleton />}>
                <WorkspaceEnvironmentsTab />
              </Suspense>
            ) : activeTab.activeSection === 'git' ? (
              <Suspense fallback={<EditorSkeleton />}>
                <WorkspaceGitTab workspaceId={activeTab.workspaceId} />
              </Suspense>
            ) : activeTab.activeSection === 'audit' ? (
              <Suspense fallback={<EditorSkeleton />}>
                <AuditLogTab />
              </Suspense>
            ) : null
          ) : isRunnerTab(activeTab) ? (
            <Suspense fallback={<EditorSkeleton />}>
              <RunnerPane tab={activeTab} groupId={node.groupId} />
            </Suspense>
          ) : isFlowTab(activeTab) ? (
            <Suspense fallback={<EditorSkeleton />}>
              <FlowPane tab={activeTab} groupId={node.groupId} />
            </Suspense>
          ) : (
            <CollectionOverviewTab tab={activeTab} />
          )
        ) : (
          <EmptyState
            variant={emptyStateVariant}
            onActivate={isInactivePaneInSplit ? () => setActiveGroup(node.groupId) : undefined}
          />
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
    </section>
  );
}
