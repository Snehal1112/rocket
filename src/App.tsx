import { listen } from '@tauri-apps/api/event';
import { type as osType } from '@tauri-apps/plugin-os';
import { useEffect, useState } from 'react';
import { Toaster } from 'sonner';
import { ErrorBoundary } from '@/components/ErrorBoundary';
import { CollectionsSidebar } from '@/components/layout/CollectionsSidebar';
import { ConsolePanel } from '@/components/layout/ConsolePanel';
import { StatusBar } from '@/components/layout/StatusBar';
import { WorkspaceToolbar } from '@/components/layout/WorkspaceToolbar';
import { PaneRenderer } from '@/components/panes/PaneRenderer';
import { SplashScreen } from '@/components/SplashScreen';
import { TitleBar } from '@/components/title-bar';
import { useKeyboardShortcuts } from '@/hooks/useKeyboardShortcuts';
import { environmentKeys } from '@/lib/queries/environment-queries';
import { workspaceKeys } from '@/lib/queries/workspace-queries';
import { getQueryClient } from '@/lib/query-client';
import { getActiveWorkspace, listWorkspaces, type Workspace } from '@/lib/tauri-api';
import { restoreUiState, scheduleSaveUiState, subscribeLayoutStoreToUiState } from '@/lib/ui-state';
import { useEnvStore } from '@/stores/env-store';
import { useLayoutStore } from '@/stores/layout-store';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import type { CollectionTab } from '@/types/pane-types';

function App() {
  const root = usePaneStore((s) => s.root);
  const [showSplash, setShowSplash] = useState(true);

  const sidebarWidth = useLayoutStore((s) => s.sidebarWidth);
  const setSidebarWidth = useLayoutStore((s) => s.setSidebarWidth);
  const isConsoleOpen = useLayoutStore((s) => s.isConsoleOpen);
  const setConsoleOpen = useLayoutStore((s) => s.setConsoleOpen);
  const consoleHeight = useLayoutStore((s) => s.consoleHeight);
  const setConsoleHeight = useLayoutStore((s) => s.setConsoleHeight);

  useKeyboardShortcuts();

  useEffect(() => {
    const init = async () => {
      // Load workspaces and the backend-persisted active workspace in parallel.
      const [workspaces, activeWs] = await Promise.all([
        listWorkspaces(),
        getActiveWorkspace().catch(() => null),
      ]);
      getQueryClient().setQueryData(workspaceKeys.all, workspaces);

      // Use the backend-persisted active workspace so the selection survives reloads.
      const persistedId = activeWs?.id ?? workspaces[0]?.id;
      if (persistedId) {
        useWorkspaceStore.getState().setActiveWorkspaceId(persistedId);
      }

      const uiState = await restoreUiState();
      if (uiState?.layoutDirection) {
        useLayoutStore.getState().setRequestLayout(uiState.layoutDirection);
      }
      if (uiState?.sidebarWidth) {
        useLayoutStore.getState().setSidebarWidth(uiState.sidebarWidth);
      }
      if (uiState?.isConsoleOpen !== undefined) {
        useLayoutStore.getState().setConsoleOpen(uiState.isConsoleOpen);
      }
      if (uiState?.consoleHeight) {
        useLayoutStore.getState().setConsoleHeight(uiState.consoleHeight);
      }

      if (uiState?.activeMode === 'workspace' && uiState.workspaceTabs) {
        const { workspaceId } = uiState.workspaceTabs;
        const ws = workspaces.find((w) => w.id === workspaceId);
        if (ws) {
          useWorkspaceStore.getState().setActiveWorkspaceId(ws.id);
          usePaneStore.getState().openWorkspaceTabs(ws.id);
        }
      } else if (uiState?.activeMode === 'collection' && uiState.activeCollection) {
        usePaneStore.getState().switchCollection(uiState.activeCollection);
        for (const saved of uiState.collectionTabs ?? []) {
          const tab: CollectionTab = {
            id: saved.id,
            title: saved.title,
            tabType: 'collection',
            collectionName: saved.collectionName,
            activeSection: (saved.activeSection as CollectionTab['activeSection']) ?? 'overview',
            isDirty: false,
          };
          usePaneStore.getState().openTab(tab);
        }
      } else if (persistedId) {
        usePaneStore.getState().openWorkspaceTabs(persistedId);
      }

      // Process env vars, global env, and collection environments are now
      // fetched automatically by TanStack Query when components mount.
    };
    void init();
  }, []);

  useEffect(() => {
    const unsubPane = usePaneStore.subscribe(scheduleSaveUiState);
    const unsubLayout = subscribeLayoutStoreToUiState();
    return () => {
      unsubPane();
      unsubLayout();
    };
  }, []);

  useEffect(() => {
    const qc = getQueryClient();
    const unsubs = Promise.all([
      listen<Workspace>('workspace-created', () => {
        qc.invalidateQueries({ queryKey: workspaceKeys.all });
      }),
      listen<Workspace>('workspace-switched', ({ payload }) => {
        useWorkspaceStore.getState().setActiveWorkspaceId(payload.id);
        usePaneStore.getState().closeAll();
        usePaneStore.getState().openWorkspaceTabs(payload.id);
        useEnvStore.getState().setActiveCollection(null);
        qc.invalidateQueries({ queryKey: workspaceKeys.all });
        qc.invalidateQueries({ queryKey: workspaceKeys.active });
        qc.invalidateQueries({ queryKey: environmentKeys.globalName });
      }),
      listen<{ id: string; newName: string }>('workspace-renamed', () => {
        qc.invalidateQueries({ queryKey: workspaceKeys.all });
      }),
      listen<{ id: string }>('workspace-closed', () => {
        qc.invalidateQueries({ queryKey: workspaceKeys.all });
      }),
      listen<{ id: string }>('workspace-deleted', () => {
        qc.invalidateQueries({ queryKey: workspaceKeys.all });
      }),
      listen<{ id: string }>('workspace-pinned', () => {
        qc.invalidateQueries({ queryKey: workspaceKeys.all });
      }),
      listen<{ id: string }>('workspace-unpinned', () => {
        qc.invalidateQueries({ queryKey: workspaceKeys.all });
      }),
      listen<{ id: string; description: string | null }>('workspace-description-updated', () => {
        qc.invalidateQueries({ queryKey: workspaceKeys.all });
      }),
    ]);

    return () => {
      unsubs.then((fns) => {
        for (const fn of fns) fn();
      });
    };
  }, []);

  useEffect(() => {
    const os = osType();
    const html = document.documentElement;
    if (os === 'linux') html.classList.add('linux');
    if (os === 'windows') html.classList.add('windows');

    if (os === 'linux' || os === 'windows') {
      const onBlur = () => html.classList.add('window-inactive');
      const onFocus = () => html.classList.remove('window-inactive');
      window.addEventListener('blur', onBlur);
      window.addEventListener('focus', onFocus);
      return () => {
        window.removeEventListener('blur', onBlur);
        window.removeEventListener('focus', onFocus);
      };
    }
  }, []);

  return (
    <div className='h-full flex flex-col overflow-hidden bg-background text-sm'>
      <TitleBar />
      <div className='flex-1 flex overflow-hidden'>
        <div
          style={{ '--sidebar-w': `${sidebarWidth}px` } as React.CSSProperties}
          className='sidebar-elevation w-(--sidebar-w) shrink-0 relative z-[5]'
        >
          <ErrorBoundary>
            <CollectionsSidebar />
          </ErrorBoundary>
        </div>
        {/* biome-ignore lint/a11y/useSemanticElements: <hr role="separator"> is a horizontal rule and cannot be a draggable, focusable resize handle */}
        <div
          role='separator'
          aria-orientation='vertical'
          aria-valuenow={sidebarWidth}
          aria-valuemin={200}
          aria-valuemax={500}
          aria-label='Resize sidebar'
          tabIndex={0}
          className='relative h-full w-px shrink-0 cursor-col-resize group z-10 overflow-visible focus-visible:outline-none focus-visible:bg-primary/60'
          onPointerDown={(e) => {
            e.preventDefault();
            const startX = e.clientX;
            const startWidth = sidebarWidth;
            const onMove = (ev: PointerEvent) => {
              const newWidth = Math.min(500, Math.max(200, startWidth + ev.clientX - startX));
              setSidebarWidth(newWidth);
            };
            const onUp = () => {
              window.removeEventListener('pointermove', onMove);
              window.removeEventListener('pointerup', onUp);
            };
            window.addEventListener('pointermove', onMove);
            window.addEventListener('pointerup', onUp);
          }}
          onKeyDown={(e) => {
            if (e.key === 'ArrowLeft') {
              e.preventDefault();
              setSidebarWidth(Math.max(200, sidebarWidth - 8));
            } else if (e.key === 'ArrowRight') {
              e.preventDefault();
              setSidebarWidth(Math.min(500, sidebarWidth + 8));
            }
          }}
        >
          <div className='w-px h-full bg-sidebar-border transition-colors group-hover:bg-primary/60' />
          {/* expanded hit area — absolutely positioned, doesn't affect layout */}
          <div className='absolute inset-y-0 -left-1 -right-1' />
        </div>
        <main className='flex-1 flex flex-col min-w-0 overflow-hidden'>
          <WorkspaceToolbar />
          <div className='flex-1 min-h-0 overflow-hidden'>
            <ErrorBoundary>
              <PaneRenderer node={root} />
            </ErrorBoundary>
          </div>
        </main>
      </div>
      <ConsolePanel
        isOpen={isConsoleOpen}
        height={consoleHeight}
        onHeightChange={setConsoleHeight}
      />
      <StatusBar
        isConsoleOpen={isConsoleOpen}
        onConsoleToggle={() => setConsoleOpen(!isConsoleOpen)}
      />
      {showSplash && <SplashScreen onComplete={() => setShowSplash(false)} />}
      <Toaster position='bottom-right' richColors closeButton />
    </div>
  );
}

export default App;
